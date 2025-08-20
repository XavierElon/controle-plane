## Starling: mini control plane in Rust

Starling is a small distributed control plane: a `controller` schedules work onto `agent`s. It teaches async Rust, HTTP APIs, traits, state machines, and observability.

### Crates

- `controller`: Axum HTTP API, scheduling loop, in-memory store for now.
- `agent`: registers, heartbeats, pulls assignments, executes commands.
- `models`: shared types (`Node`, `Job`, `Task`, etc.) with Serde.
- `storage`: store trait; in-memory impl currently embedded in controller.

### Requirements

- Rust (stable), Cargo
- macOS or Linux
- curl

### Quick start (script)

```bash
# from repo root
chmod +x scripts/dev-up.sh
./scripts/dev-up.sh
```

The script:

- Builds the workspace
- Starts the controller in background (logs to `logs/controller.log`)
- Detects the listening port from the controller’s startup log
- Starts the agent pointing at the detected URL (logs to `logs/agent.log`)
- Tails both logs

Stop with Ctrl-C; the script will clean up processes.

### Submit a job

In a new terminal, adjust the port if needed (the script prints it):

```bash
PORT=8080  # or what the script printed
curl -s -X POST "http://127.0.0.1:${PORT}/v1/jobs" \
  -H 'content-type: application/json' \
  -d '{"name":"echo-hello","replicas":2,"command":["echo","hello from starling"],"constraints":[],"priority":5}'
```

The agent should log receiving the assignment and print the command output.

### Manual run (without the script)

Controller:

```bash
cd controller
cargo run
# it prints: "Starting controller on 0.0.0.0:<PORT>"
```

Agent (set `CONTROLLER_URL` to the controller’s port):

```bash
cd ../agent
CONTROLLER_URL=http://127.0.0.1:<PORT> cargo run
```

### Troubleshooting

- 405 Method Not Allowed on register:
  - You’re hitting the wrong service on that port. Make sure the controller is running and you’re posting to `/v1/nodes/register`.
- Address already in use:
  - Free the port or change the controller bind port in `controller/src/main.rs`.
- Agent “error decoding response body”:
  - Usually a non-JSON response (404/500). Check controller logs and route paths.
- Timestamps look odd:
  - They’re `OffsetDateTime`. To print RFC3339, add `#[serde(with = "time::serde::rfc3339")]` on `last_heartbeat`.

### Next steps

- Add CAS (compare-and-set) for atomic assignment.
- Move in-memory store to `storage`, add `sled` or `sqlx` Postgres backend.
- Add metrics and structured tracing.
