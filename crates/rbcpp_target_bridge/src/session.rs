// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use iec_ir::Value;
use rbcpp_target::{
    parse_ide_mapping, FileBackedHal, IdeMappingEntry, IoDirection, IoEncoding, IoSymbol,
    ModbusArea, ModbusPoint, ModbusTransport,
};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};

use crate::modbus_transport::{SimModbusTransport, TcpModbusTransport};
use crate::{probe_tcp, resolve_host_port, VERSION};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetMode {
    #[default]
    Offline,
    File,
    ModbusSim,
    ModbusTcp,
    HybridFileModbus,
}

#[derive(Debug, Default)]
pub struct SessionState {
    mode: TargetMode,
    label: String,
    address: String,
    endpoint: String,
    project_id: Option<String>,
    workspace_root: PathBuf,
    mapping_text: String,
    program_hash: Option<String>,
    deploy_hash: Option<String>,
    runtime_version: Option<String>,
    last_error: Option<String>,
    running: bool,
    file_hal: FileBackedHal,
    file_symbols: BTreeMap<String, IoSymbol>,
    file_paths: BTreeMap<String, PathBuf>,
    file_encodings: BTreeMap<String, IoEncoding>,
    modbus_symbols: BTreeMap<String, ModbusPoint>,
    modbus_sim: Option<SimModbusTransport>,
    modbus_tcp: Option<TcpModbusTransport>,
}

#[derive(Debug, Deserialize)]
pub struct ConnectRequest {
    pub address: String,
    pub port: Option<u16>,
    pub project_id: String,
    pub mapping_text: String,
    pub workspace_root: Option<String>,
    pub program_hash: Option<String>,
    pub simulate: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ControlRequest {
    pub action: String,
}

impl SessionState {
    pub fn disconnect(&mut self) {
        *self = Self {
            workspace_root: self.workspace_root.clone(),
            ..Self::default()
        };
    }

    pub fn status_json(&self) -> JsonValue {
        json!({
            "ok": self.mode != TargetMode::Offline,
            "state": self.connection_state(),
            "mode": mode_label(self.mode),
            "label": self.label,
            "address": self.address,
            "endpoint": self.endpoint,
            "projectId": self.project_id,
            "runtimeVersion": self.runtime_version,
            "programHash": self.program_hash,
            "deployHash": self.deploy_hash,
            "bindingCount": self.file_symbols.len() + self.modbus_symbols.len(),
            "running": self.running,
            "lastError": self.last_error,
            "editorMatchesTarget": self.program_hash.is_some(),
        })
    }

    pub fn connect(&mut self, request: ConnectRequest) -> Result<JsonValue, String> {
        self.disconnect();
        let port = request.port.unwrap_or(502);
        let endpoint = resolve_host_port(&request.address, port)?;
        let simulate = request
            .simulate
            .unwrap_or(request.address.starts_with("sim://"));
        let workspace_root =
            resolve_workspace_root(request.workspace_root.as_deref(), &request.project_id)?;
        fs::create_dir_all(&workspace_root).map_err(|error| {
            format!(
                "failed to create workspace root {}: {error}",
                workspace_root.display()
            )
        })?;
        fs::create_dir_all(workspace_root.join("io"))
            .map_err(|error| format!("failed to create io directory: {error}"))?;

        let entries = parse_ide_mapping(&workspace_root, &request.mapping_text);
        if entries.is_empty() {
            return Err("mapping has no supported file or modbus bindings".to_string());
        }

        let mut file_hal = FileBackedHal::new();
        let mut file_symbols = BTreeMap::new();
        let mut file_paths = BTreeMap::new();
        let mut file_encodings = BTreeMap::new();
        let mut modbus_symbols = BTreeMap::new();
        for entry in entries {
            match entry {
                IdeMappingEntry::File { symbol, binding } => {
                    let key = symbol_key(&symbol);
                    file_hal =
                        file_hal.bind_name(symbol.clone(), binding.path.clone(), binding.encoding);
                    file_paths.insert(key.clone(), binding.path);
                    file_encodings.insert(key.clone(), binding.encoding);
                    file_symbols.insert(
                        key,
                        IoSymbol::new(symbol.clone(), symbol, IoDirection::Unknown, "BOOL"),
                    );
                }
                IdeMappingEntry::Modbus { symbol, point } => {
                    modbus_symbols.insert(symbol_key(&symbol), point);
                }
            }
        }

        if file_symbols.is_empty() && modbus_symbols.is_empty() {
            return Err("mapping has no supported file or modbus bindings".to_string());
        }

        if !modbus_symbols.is_empty() {
            if simulate {
                self.modbus_sim = Some(SimModbusTransport::new());
                self.mode = if file_symbols.is_empty() {
                    TargetMode::ModbusSim
                } else {
                    TargetMode::HybridFileModbus
                };
                self.label = if simulate {
                    "Modbus simulator".to_string()
                } else {
                    "Modbus TCP".to_string()
                };
            } else {
                probe_tcp(&endpoint)?;
                self.modbus_tcp = Some(TcpModbusTransport::new(endpoint.clone()));
                self.mode = if file_symbols.is_empty() {
                    TargetMode::ModbusTcp
                } else {
                    TargetMode::HybridFileModbus
                };
                self.label = "Modbus TCP".to_string();
            }
        } else {
            self.mode = TargetMode::File;
            self.label = "File-backed I/O".to_string();
        }

        self.address = request.address;
        self.endpoint = if modbus_symbols.is_empty() {
            workspace_root.display().to_string()
        } else {
            endpoint
        };
        self.project_id = Some(request.project_id);
        self.workspace_root = workspace_root;
        self.mapping_text = request.mapping_text;
        self.program_hash = request.program_hash;
        self.runtime_version = Some(format!("rbcpp-target-bridge/{VERSION}"));
        self.file_hal = file_hal;
        self.file_symbols = file_symbols;
        self.file_paths = file_paths;
        self.file_encodings = file_encodings;
        self.modbus_symbols = modbus_symbols;
        self.seed_missing_file_bindings()?;
        self.last_error = None;
        self.running = false;

        let io = self.read_io()?;
        Ok(json!({
            "ok": true,
            "state": self.connection_state(),
            "mode": mode_label(self.mode),
            "runtimeVersion": self.runtime_version,
            "bindingCount": self.file_symbols.len() + self.modbus_symbols.len(),
            "values": io,
        }))
    }

    fn seed_missing_file_bindings(&mut self) -> Result<(), String> {
        for (key, io_symbol) in self.file_symbols.clone() {
            let Some(path) = self.file_paths.get(&key) else {
                continue;
            };
            if path.exists() {
                continue;
            }
            let encoding = self
                .file_encodings
                .get(&key)
                .copied()
                .unwrap_or(IoEncoding::Decimal);
            let default_value = match encoding {
                IoEncoding::Bool01 => Value::Bool(false),
                IoEncoding::Decimal => Value::Int(0),
                IoEncoding::Text => Value::String(String::new()),
            };
            self.file_hal
                .write(&io_symbol, &default_value)
                .map_err(|error| format!("failed to seed file binding for {key}: {error}"))?;
        }
        Ok(())
    }

    pub fn control(&mut self, action: &str) -> Result<JsonValue, String> {
        if self.mode == TargetMode::Offline {
            return Err("target is offline".to_string());
        }
        match action {
            "run" => self.running = true,
            "stop" => self.running = false,
            "reset" => {
                self.running = false;
                if let Some(sim) = self.modbus_sim.as_mut() {
                    *sim = SimModbusTransport::new();
                }
            }
            other => return Err(format!("unknown control action '{other}'")),
        }
        Ok(self.status_json())
    }

    pub fn deploy(&mut self, request: DeployRequestBody) -> Result<JsonValue, String> {
        let workspace_root =
            resolve_workspace_root(request.workspace_root.as_deref(), &request.project_id)?;
        fs::create_dir_all(&workspace_root)
            .map_err(|error| format!("failed to create workspace root: {error}"))?;
        fs::create_dir_all(workspace_root.join("target"))
            .map_err(|error| format!("failed to create target directory: {error}"))?;
        fs::write(
            workspace_root.join("target/mapping.toml"),
            request.mapping_text.as_bytes(),
        )
        .map_err(|error| format!("failed to write mapping.toml: {error}"))?;
        if let Some(generated_c) = request.generated_c {
            fs::create_dir_all(workspace_root.join("build"))
                .map_err(|error| format!("failed to create build directory: {error}"))?;
            fs::write(
                workspace_root.join("build/generated.c"),
                generated_c.as_bytes(),
            )
            .map_err(|error| format!("failed to write generated.c: {error}"))?;
        }
        if let Some(deploy_package) = request.deploy_package {
            fs::write(
                workspace_root.join("target/deploy-package.json"),
                deploy_package.as_bytes(),
            )
            .map_err(|error| format!("failed to write deploy package: {error}"))?;
        }
        let deploy_hash = format!("deploy-{}", request.project_id);
        self.deploy_hash = Some(deploy_hash.clone());
        if let Some(program_hash) = request.program_hash {
            self.program_hash = Some(program_hash);
        }
        self.mapping_text = request.mapping_text;
        Ok(json!({
            "ok": true,
            "deployHash": deploy_hash,
            "workspaceRoot": workspace_root,
        }))
    }

    pub fn read_io(&self) -> Result<Vec<JsonValue>, String> {
        let mut values = Vec::new();
        for (symbol, io_symbol) in &self.file_symbols {
            let value = self
                .file_hal
                .read(io_symbol)
                .map_err(|error| format!("failed to read file binding for {symbol}: {error}"))?
                .unwrap_or(Value::Bool(false));
            values.push(json!({
                "symbol": symbol,
                "kind": "file",
                "target": self.file_paths.get(symbol).map(|path| path.display().to_string()),
                "value": value_to_json(&value),
            }));
        }

        for (symbol, point) in &self.modbus_symbols {
            let value = self
                .read_modbus_point(*point)
                .map_err(|error| format!("failed to read modbus binding for {symbol}: {error}"))?;
            values.push(json!({
                "symbol": symbol,
                "kind": "modbus",
                "target": format!("{}:{}:{}", point.unit_id, area_label(point.area), point.address),
                "value": value,
            }));
        }
        Ok(values)
    }

    pub fn write_io(&mut self, symbol: &str, raw: &JsonValue) -> Result<(), String> {
        let key = symbol_key(symbol);
        if let Some(io_symbol) = self.file_symbols.get(&key).cloned() {
            let encoding = self
                .file_encodings
                .get(&key)
                .copied()
                .unwrap_or(IoEncoding::Decimal);
            let value = json_to_value(raw, encoding)?;
            self.file_hal
                .write(&io_symbol, &value)
                .map_err(|error| format!("failed to write file binding for {symbol}: {error}"))?;
            return Ok(());
        }
        let point = self
            .modbus_symbols
            .get(&key)
            .copied()
            .ok_or_else(|| format!("unknown binding '{symbol}'"))?;
        self.write_modbus_point(point, raw)?;
        Ok(())
    }

    fn connection_state(&self) -> &'static str {
        if self.mode == TargetMode::Offline {
            return "offline";
        }
        if self.running {
            return "running";
        }
        "online"
    }

    fn read_modbus_point(&self, point: ModbusPoint) -> Result<JsonValue, String> {
        if let Some(transport) = self.modbus_tcp.as_ref() {
            return read_point(transport, point);
        }
        if let Some(transport) = self.modbus_sim.as_ref() {
            return read_point(transport, point);
        }
        Err("session is not using modbus transport".to_string())
    }

    fn write_modbus_point(&mut self, point: ModbusPoint, raw: &JsonValue) -> Result<(), String> {
        if let Some(transport) = self.modbus_tcp.as_mut() {
            return write_point(transport, point, raw);
        }
        if let Some(transport) = self.modbus_sim.as_mut() {
            return write_point(transport, point, raw);
        }
        Err("session is not using modbus transport".to_string())
    }
}

#[derive(Debug, Deserialize)]
pub struct DeployRequestBody {
    pub project_id: String,
    pub mapping_text: String,
    pub workspace_root: Option<String>,
    pub deploy_package: Option<String>,
    pub generated_c: Option<String>,
    pub program_hash: Option<String>,
}

fn resolve_workspace_root(
    workspace_root: Option<&str>,
    project_id: &str,
) -> Result<PathBuf, String> {
    if let Some(root) = workspace_root {
        if !root.trim().is_empty() {
            return Ok(PathBuf::from(root));
        }
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    Ok(Path::new(&home)
        .join(".robocpp")
        .join("studio-target")
        .join(project_id))
}

fn mode_label(mode: TargetMode) -> &'static str {
    match mode {
        TargetMode::Offline => "offline",
        TargetMode::File => "file",
        TargetMode::ModbusSim => "modbus-sim",
        TargetMode::ModbusTcp => "modbus-tcp",
        TargetMode::HybridFileModbus => "hybrid",
    }
}

fn symbol_key(input: &str) -> String {
    input.trim().to_ascii_uppercase()
}

fn area_label(area: ModbusArea) -> &'static str {
    match area {
        ModbusArea::Coil => "coil",
        ModbusArea::DiscreteInput => "discrete_input",
        ModbusArea::HoldingRegister => "holding_register",
        ModbusArea::InputRegister => "input_register",
    }
}

fn read_point<T: ModbusTransport>(transport: &T, point: ModbusPoint) -> Result<JsonValue, String> {
    let value = match point.area {
        ModbusArea::Coil => JsonValue::Bool(
            transport
                .read_coil(point.unit_id, point.address)
                .map_err(|error| error.to_string())?,
        ),
        ModbusArea::DiscreteInput => JsonValue::Bool(
            transport
                .read_discrete_input(point.unit_id, point.address)
                .map_err(|error| error.to_string())?,
        ),
        ModbusArea::HoldingRegister => JsonValue::Number(
            transport
                .read_holding_register(point.unit_id, point.address)
                .map_err(|error| error.to_string())?
                .into(),
        ),
        ModbusArea::InputRegister => JsonValue::Number(
            transport
                .read_input_register(point.unit_id, point.address)
                .map_err(|error| error.to_string())?
                .into(),
        ),
    };
    Ok(value)
}

fn write_point<T: ModbusTransport>(
    transport: &mut T,
    point: ModbusPoint,
    raw: &JsonValue,
) -> Result<(), String> {
    match point.area {
        ModbusArea::Coil => {
            transport
                .write_coil(point.unit_id, point.address, raw.as_bool().unwrap_or(false))
                .map_err(|error| error.to_string())?;
        }
        ModbusArea::HoldingRegister => {
            let value = raw
                .as_u64()
                .or_else(|| raw.as_i64().map(|entry| entry.max(0) as u64))
                .unwrap_or(0)
                .min(u16::MAX as u64) as u16;
            transport
                .write_holding_register(point.unit_id, point.address, value)
                .map_err(|error| error.to_string())?;
        }
        ModbusArea::DiscreteInput | ModbusArea::InputRegister => {
            return Err("input-only modbus area cannot be written".to_string());
        }
    }
    Ok(())
}

fn value_to_json(value: &Value) -> JsonValue {
    match value {
        Value::Bool(flag) => JsonValue::Bool(*flag),
        Value::Int(number) => JsonValue::Number((*number).into()),
        Value::Real(number) => {
            JsonValue::Number(serde_json::Number::from_f64(*number).unwrap_or(0.into()))
        }
        Value::String(text) | Value::WString(text) => JsonValue::String(text.clone()),
        Value::Unit => JsonValue::Null,
        Value::TimeMs(number) => JsonValue::Number((*number as i64).into()),
        Value::Array(values) => JsonValue::Array(values.iter().map(value_to_json).collect()),
        Value::Struct(fields) => {
            let mut object = serde_json::Map::new();
            for (key, value) in fields {
                object.insert(key.clone(), value_to_json(value));
            }
            JsonValue::Object(object)
        }
    }
}

fn json_to_value(raw: &JsonValue, encoding: IoEncoding) -> Result<Value, String> {
    match encoding {
        IoEncoding::Bool01 => Ok(Value::Bool(raw.as_bool().unwrap_or(false))),
        IoEncoding::Text => Ok(Value::String(
            raw.as_str()
                .map(str::to_string)
                .unwrap_or_else(|| raw.to_string()),
        )),
        IoEncoding::Decimal => {
            if let Some(number) = raw.as_i64() {
                return Ok(Value::Int(number));
            }
            if let Some(number) = raw.as_u64() {
                return Ok(Value::Int(number as i64));
            }
            if let Some(text) = raw.as_str() {
                let number = text
                    .parse::<i64>()
                    .map_err(|error| format!("invalid decimal value '{text}': {error}"))?;
                return Ok(Value::Int(number));
            }
            Err("expected a numeric file binding value".to_string())
        }
    }
}
