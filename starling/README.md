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

### Testing

- Start Postgres:

```bash
docker compose up -d postgres
```

- Export env and run migration:

```bash
export TEST_DATABASE_URL=postgres://starling:starling@127.0.0.1:5437/starling
export DATABASE_URL="$TEST_DATABASE_URL"
cargo run -p storage --bin migrate
```

- Run tests:

```bash
cargo test --workspace
# or per crate
cargo test -p models
cargo test -p storage
```

- If `models` tests fail on `serde_json`, add:

```toml
# models/Cargo.toml
[dev-dependencies]
serde_json = "1"
```

### Error handling overview

- storage (`storage/src/error.rs`): defines DB-layer errors

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("db error: {0}")] Db(#[from] sqlx::Error),
    #[error("serde error: {0}")] Serde(#[from] serde_json::Error),
}
pub type StorageResult<T> = Result<T, StorageError>;
```

- controller (`controller/src/error.rs`): maps errors to HTTP responses

```rust
use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde::Serialize;
use storage::error::StorageError;

#[derive(Debug)]
pub enum ApiError { NotFound, BadRequest(String), Storage(StorageError), Internal(String) }

impl From<StorageError> for ApiError { fn from(e: StorageError) -> Self { ApiError::Storage(e) } }

#[derive(Serialize)] struct ErrorBody { message: String }

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, Json(ErrorBody { message: "not found".into() })).into_response(),
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, Json(ErrorBody { message: m })).into_response(),
            ApiError::Storage(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorBody { message: e.to_string() })).into_response(),
            ApiError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorBody { message: m })).into_response(),
        }
    }
}
```

- Handler pattern (example):

```rust
// controller/src/main.rs
mod error;
use crate::error::ApiError;

async fn heartbeat(...) -> Result<StatusCode, ApiError> {
    let Some(mut node) = state.store.get_node(id).await? else { return Err(ApiError::NotFound); };
    node.last_heartbeat = OffsetDateTime::now_utc();
    state.store.put_node(&node).await?;
    Ok(StatusCode::NO_CONTENT)
}
```

### API quickstart (curl)

Set base URL:

```bash
BASE=http://127.0.0.1:8090
```

Register a node (captures NODE_ID):

```bash
NODE_ID=$(
  curl -s -X POST "$BASE/v1/nodes/register" \
    -H 'content-type: application/json' \
    -d '{"hostname":"agent-1","labels":[["region","us-east-1"],["role","worker"]]}' \
  | jq -r '.id'
)
echo "NODE_ID=$NODE_ID"
```

Heartbeat:

```bash
curl -i -X POST "$BASE/v1/nodes/$NODE_ID/heartbeat"
```

Submit a job (captures JOB_ID):

```bash
JOB_ID=$(
  curl -s -X POST "$BASE/v1/jobs" \
    -H 'content-type: application/json' \
    -d '{"name":"echo-hello","replicas":2,"command":["echo","hello from starling"],"constraints":[["region","us-east-1"]],"priority":5}' \
  | jq -r '.id'
)
echo "JOB_ID=$JOB_ID"
```

Agent pulls next assignment:

```bash
ASSIGN=$(curl -s "$BASE/v1/agents/$NODE_ID/next")
echo "$ASSIGN" | jq
TASK_ID=$(echo "$ASSIGN" | jq -r '.task_id')
```

Update task status:

```bash
# Assigned
curl -i -X POST "$BASE/v1/tasks/status" \
  -H 'content-type: application/json' \
  -d "{\"task_id\":\"$TASK_ID\",\"state\":{\"Assigned\":{\"node_id\":\"$NODE_ID\"}}}"

# Running
NOW=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
curl -i -X POST "$BASE/v1/tasks/status" \
  -H 'content-type: application/json' \
  -d "{\"task_id\":\"$TASK_ID\",\"state\":{\"Running\":{\"node_id\":\"$NODE_ID\",\"started_at\":\"$NOW\"}}}"

# Succeeded
FIN=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
curl -i -X POST "$BASE/v1/tasks/status" \
  -H 'content-type: application/json' \
  -d "{\"task_id\":\"$TASK_ID\",\"state\":{\"Succeeded\":{\"finished_at\":\"$FIN\"}}}"
```

````

### Print commands on controller startup
Add this helper in `controller/src/main.rs` (anywhere above `main` is fine):

```rust
fn print_quickstart(addr: std::net::SocketAddr) {
	let base = format!("http://127.0.0.1:{}", addr.port());
	tracing::info!(
		"\nAPI quickstart (curl)\n\
		\n# Set base\nBASE={base}\
		\n\n# Register a node\nNODE_ID=$(curl -s -X POST \"$BASE/v1/nodes/register\" -H 'content-type: application/json' -d '{{\"hostname\":\"agent-1\",\"labels\":[[\"region\",\"us-east-1\"],[\"role\",\"worker\"]]}}' | jq -r '.id')\
		\n\n# Heartbeat\ncurl -i -X POST \"$BASE/v1/nodes/$NODE_ID/heartbeat\"\
		\n\n# Submit a job\nJOB_ID=$(curl -s -X POST \"$BASE/v1/jobs\" -H 'content-type: application/json' -d '{{\"name\":\"echo-hello\",\"replicas\":2,\"command\":[\"echo\",\"hello from starling\"],\"constraints\":[[\"region\",\"us-east-1\"]],\"priority\":5}}' | jq -r '.id')\
		\n\n# Agent pulls next assignment\nASSIGN=$(curl -s \"$BASE/v1/agents/$NODE_ID/next\"); echo \"$ASSIGN\" | jq; TASK_ID=$(echo \"$ASSIGN\" | jq -r '.task_id')\
		\n\n# Update task to Assigned\ncurl -i -X POST \"$BASE/v1/tasks/status\" -H 'content-type: application/json' -d \"{{\\\"task_id\\\":\\\"$TASK_ID\\\",\\\"state\\\":{{\\\"Assigned\\\":{{\\\"node_id\\\":\\\"$NODE_ID\\\"}}}}}}\"\
		\n"
	);
}
````

Then call it when starting the server (split your current one-liner so it’s readable):

```53:56:starling/controller/src/main.rs
	let addr: SocketAddr = "0.0.0.0:8090".parse().unwrap();
	info!("Starting controller on {}", addr);
	print_quickstart(addr);
	let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
	axum::serve(listener, app).await.unwrap();
```

- This prints a ready-to-copy block with the correct port on startup.
- The README section gives the same commands for reference.

- I added a paste-ready README section and a small `print_quickstart` helper in `controller/src/main.rs`, then invoked it at startup so the server prints the curl steps with the actual port.
