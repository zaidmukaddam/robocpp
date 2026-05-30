# Target Bridge Setup

RoboC++ Studio talks to hardware and file-backed I/O through the local
`rbcpp_target_bridge` HTTP service. The web IDE proxies requests from
`/target-bridge` to `http://127.0.0.1:8787` during development.

## Quick start (simulator + bridge)

From `ide/web`:

```sh
npm run dev:with-target
```

This starts the bridge and the Vite dev server together. Open
`http://127.0.0.1:5173/` and pick **Simulator** in the target bar.

## Manual bridge start

From the repository root:

```sh
cargo run -p rbcpp_target_bridge
```

Optional bind address:

```sh
cargo run -p rbcpp_target_bridge -- --bind 127.0.0.1:8787
```

Then run the IDE separately:

```sh
cd ide/web && npm run dev
```

Set **Target bridge URL** in IDE Settings to match the bind address
(default `http://127.0.0.1:8787`).

## Simulator target

1. Open a project and run **Check project**.
2. Choose **Simulator** in the target connection bar.
3. Use **Run** for local simulation, or connect the bridge for deploy
   package staging on disk.

The simulator does not require Modbus. It uses the in-browser runner and
debug trace panels.

## Hardware target (Modbus TCP)

1. Start `rbcpp_target_bridge` on a machine that can reach the PLC.
2. Configure target mappings with Modbus addresses such as `1:coil:0`.
3. Set **Target bridge URL** in IDE Settings.
4. Choose **Hardware** and connect.
5. Review deploy validation in the target inspector before download.

Safety-gated actions (download, run, stop, reset, force, write) show a
confirmation dialog with program hash, deploy hash, and consequences.

## Failure modes

| Symptom | Likely cause | What to try |
| --- | --- | --- |
| Target stays offline | Bridge not running | Start `rbcpp_target_bridge` or `npm run dev:with-target` |
| Connect hangs on connecting | Wrong bridge URL or firewall | Verify Settings URL and `curl http://127.0.0.1:8787/health` |
| Deploy validation fails | Missing mappings or stale generated C | Run Check, Build C, review target mapping coverage |
| Modbus read/write errors | Invalid unit/register/address | Use mapping form hints (`1:coil:0` format) |
| Editor/target mismatch | Source changed after download | Re-check project and redeploy |

## Workspace root

File-backed deploy artifacts and adapter outputs use
`~/.robocpp/studio-target` by default. Override **Target workspace root**
in IDE Settings when running the bridge on another host.
