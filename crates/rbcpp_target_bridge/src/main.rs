// SPDX-License-Identifier: MIT OR Apache-2.0

mod modbus_transport;
mod session;

use std::env;
use std::net::ToSocketAddrs;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use serde::Deserialize;
use serde_json::json;
use session::{ConnectRequest, ControlRequest, DeployRequestBody, SessionState};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let mut bind = "127.0.0.1:8787".to_string();
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bind" => {
                bind = args.next().ok_or("missing value for --bind")?;
            }
            "-h" | "--help" => {
                print_usage();
                return Ok(());
            }
            other => return Err(format!("unknown argument '{other}'")),
        }
    }

    let server = Server::http(&bind).map_err(|error| format!("failed to bind {bind}: {error}"))?;
    eprintln!("rbcpp-target-bridge {VERSION} listening on http://{bind}");
    let state = Arc::new(Mutex::new(SessionState::default()));

    for request in server.incoming_requests() {
        let state = Arc::clone(&state);
        if let Err(error) = dispatch_request(request, state) {
            eprintln!("request error: {error}");
        }
    }

    Ok(())
}

fn print_usage() {
    eprintln!("rbcpp-target-bridge\n\nUsage:\n  rbcpp-target-bridge [--bind 127.0.0.1:8787]\n");
}

fn dispatch_request(request: Request, state: Arc<Mutex<SessionState>>) -> Result<(), String> {
    handle_request(request, state)
}

fn handle_request(mut request: Request, state: Arc<Mutex<SessionState>>) -> Result<(), String> {
    if request.method() == &Method::Options {
        return write_json(request, StatusCode(204), json!({ "ok": true }));
    }

    let path = request.url().to_string();
    let path = path.split('?').next().unwrap_or(&path);

    match (request.method(), path) {
        (&Method::Get, "/health") => write_json(
            request,
            StatusCode(200),
            json!({
                "ok": true,
                "service": "rbcpp-target-bridge",
                "version": VERSION,
            }),
        ),
        (&Method::Get, "/api/v1/session") => {
            let session = state
                .lock()
                .map_err(|_| "session lock poisoned".to_string())?;
            write_json(request, StatusCode(200), session.status_json())
        }
        (&Method::Post, "/api/v1/session") => {
            let body = read_body(&mut request)?;
            let connect: ConnectRequest = serde_json::from_str(&body)
                .map_err(|error| format!("invalid connect body: {error}"))?;
            let mut session = state
                .lock()
                .map_err(|_| "session lock poisoned".to_string())?;
            match session.connect(connect) {
                Ok(status) => write_json(request, StatusCode(200), status),
                Err(error) => write_json(
                    request,
                    StatusCode(400),
                    json!({ "ok": false, "error": error }),
                ),
            }
        }
        (&Method::Delete, "/api/v1/session") => {
            let mut session = state
                .lock()
                .map_err(|_| "session lock poisoned".to_string())?;
            session.disconnect();
            write_json(
                request,
                StatusCode(200),
                json!({ "ok": true, "state": "offline" }),
            )
        }
        (&Method::Get, "/api/v1/io") => {
            let session = state
                .lock()
                .map_err(|_| "session lock poisoned".to_string())?;
            match session.read_io() {
                Ok(values) => write_json(request, StatusCode(200), json!({ "values": values })),
                Err(error) => write_json(
                    request,
                    StatusCode(400),
                    json!({ "ok": false, "error": error }),
                ),
            }
        }
        (&Method::Post, "/api/v1/io") => {
            let body = read_body(&mut request)?;
            let write: IoWriteRequest = serde_json::from_str(&body)
                .map_err(|error| format!("invalid io write body: {error}"))?;
            let mut session = state
                .lock()
                .map_err(|_| "session lock poisoned".to_string())?;
            match session.write_io(&write.symbol, &write.value) {
                Ok(()) => write_json(request, StatusCode(200), json!({ "ok": true })),
                Err(error) => write_json(
                    request,
                    StatusCode(400),
                    json!({ "ok": false, "error": error }),
                ),
            }
        }
        (&Method::Post, "/api/v1/session/control") => {
            let body = read_body(&mut request)?;
            let control: ControlRequest = serde_json::from_str(&body)
                .map_err(|error| format!("invalid control body: {error}"))?;
            let mut session = state
                .lock()
                .map_err(|_| "session lock poisoned".to_string())?;
            match session.control(control.action.as_str()) {
                Ok(status) => write_json(request, StatusCode(200), status),
                Err(error) => write_json(
                    request,
                    StatusCode(400),
                    json!({ "ok": false, "error": error }),
                ),
            }
        }
        (&Method::Post, "/api/v1/deploy") => {
            let body = read_body(&mut request)?;
            let deploy: DeployRequestBody = serde_json::from_str(&body)
                .map_err(|error| format!("invalid deploy body: {error}"))?;
            let mut session = state
                .lock()
                .map_err(|_| "session lock poisoned".to_string())?;
            match session.deploy(deploy) {
                Ok(result) => write_json(request, StatusCode(200), result),
                Err(error) => write_json(
                    request,
                    StatusCode(400),
                    json!({ "ok": false, "error": error }),
                ),
            }
        }
        _ => write_json(
            request,
            StatusCode(404),
            json!({ "ok": false, "error": "not found" }),
        ),
    }
}

#[derive(Debug, Deserialize)]
struct IoWriteRequest {
    symbol: String,
    value: serde_json::Value,
}

fn read_body(request: &mut Request) -> Result<String, String> {
    let mut body = String::new();
    request
        .as_reader()
        .read_to_string(&mut body)
        .map_err(|error| format!("failed to read body: {error}"))?;
    Ok(body)
}

fn write_json(
    request: Request,
    status: StatusCode,
    payload: serde_json::Value,
) -> Result<(), String> {
    let body = serde_json::to_string(&payload).map_err(|error| error.to_string())?;
    let mut response = Response::from_string(body).with_status_code(status);
    response = add_cors(response);
    request
        .respond(response)
        .map_err(|error| format!("failed to write response: {error}"))
}

fn add_cors(
    mut response: Response<std::io::Cursor<Vec<u8>>>,
) -> Response<std::io::Cursor<Vec<u8>>> {
    response.add_header(Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap());
    response.add_header(
        Header::from_bytes("Access-Control-Allow-Methods", "GET, POST, DELETE, OPTIONS").unwrap(),
    );
    response
        .add_header(Header::from_bytes("Access-Control-Allow-Headers", "Content-Type").unwrap());
    response
}

pub fn resolve_host_port(address: &str, port: u16) -> Result<String, String> {
    if address.starts_with("sim://") {
        return Ok(format!("127.0.0.1:{port}"));
    }
    if let Some((host, explicit_port)) = address.rsplit_once(':') {
        if explicit_port.parse::<u16>().is_ok() {
            return Ok(address.to_string());
        }
        let _ = host;
    }
    Ok(format!("{address}:{port}"))
}

pub fn probe_tcp(address: &str) -> Result<(), String> {
    if address.starts_with("sim://") {
        return Ok(());
    }
    let socket_addr = address
        .to_socket_addrs()
        .map_err(|error| format!("invalid target address '{address}': {error}"))?
        .next()
        .ok_or_else(|| format!("could not resolve target address '{address}'"))?;
    std::net::TcpStream::connect_timeout(&socket_addr, std::time::Duration::from_secs(3))
        .map_err(|error| format!("could not reach {address}: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_host_port_appends_default_modbus_port() {
        assert_eq!(
            resolve_host_port("192.168.0.10", 502).unwrap(),
            "192.168.0.10:502"
        );
        assert_eq!(
            resolve_host_port("192.168.0.10:1502", 502).unwrap(),
            "192.168.0.10:1502"
        );
    }
}
