mod error;
use crate::error::ApiError;
use axum::{
	extract::{Path, State},
	http::StatusCode,
	routing::{get, post},
	Json, Router
};
use dashmap::DashMap;
use models::*;
use std::{net::SocketAddr, sync::Arc};
use time::OffsetDateTime;
use tokio::sync::mpsc;
use tracing::info;
use uuid::Uuid;
use dotenvy;

#[derive(Clone)]
struct AppState {
    store: storage::DynStore,
    assignment_queues: Arc<DashMap<Uuid, mpsc::Sender<Assignment>>>,
    lease_ttl_secs: u64
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .pretty()
        .init();

     // Read DATABASE_URL from env
	 let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
	 let pool = storage::create_pool(&database_url).await;
	 let store = Arc::new(storage::SqlxStore::new(pool)) as storage::DynStore;
 
	 let app_state = AppState {
		 store,
		 assignment_queues: Arc::new(DashMap::new()),
		 lease_ttl_secs: 30,
	 };

    let app = Router::new()
        .route("/v1/nodes/register", post(register_node))
        .route("/v1/nodes/:id/heartbeat", post(heartbeat))
        .route("/v1/jobs", post(create_job))
        .route("/v1/agents/:id/next", get(next_assignment))
        .route("/v1/tasks/status", post(update_task_status))
        .with_state(app_state);

	let addr: SocketAddr = "0.0.0.0:8090".parse().unwrap();
	info!("Starting controller on {}", addr);
    print_quickstart(addr);
	let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
	axum::serve(listener, app).await.unwrap();
}

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

async fn register_node(
    State(state): State<AppState>,
    Json(req): Json<NodeRegistration>,
) -> Result<Json<Node>, ApiError> {
    let node = Node {
        id: Uuid::new_v4(),
        hostname: req.hostname,
        labels: req.labels,
        last_heartbeat: OffsetDateTime::now_utc(),
        lease_ttl_secs: state.lease_ttl_secs,
    };
    state.store.put_node(&node).await?;
    let (tx, _rx) = mpsc::channel::<Assignment>(128);
    state.assignment_queues.insert(node.id, tx);
    Ok(Json(node))
}

async fn heartbeat(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let Some(mut node) = state.store.get_node(id).await? else {
        return Err(ApiError::NotFound);
    };
    node.last_heartbeat = OffsetDateTime::now_utc();
    state.store.put_node(&node).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn create_job(
    State(state): State<AppState>,
    Json(spec): Json<JobSpec>,
) -> Result<Json<Job>, ApiError> {
    if spec.name.trim().is_empty() || spec.command.is_empty() {
        return Err(ApiError::BadRequest("name and command are required".into()));
    }
    let job = Job { id: Uuid::new_v4(), spec, created_at: OffsetDateTime::now_utc() };
    state.store.put_job(&job).await?;

    // Naive: create N pending tasks and store them
    for _ in 0..job.spec.replicas {
        let t = Task {
            id: Uuid::new_v4(),
            job_id: job.id,
            state: TaskState::Pending,
            attempt: 0,
            priority: job.spec.priority,
        };
        state.store.put_task(&t).await?;
    }

    Ok(Json(job))
}

async fn next_assignment(
    State(state): State<AppState>,
    Path(node_id): Path<Uuid>,
) -> Result<Json<Option<Assignment>>, ApiError> {
    // Gather all pending tasks across all jobs
    let mut tasks = Vec::new();
    for job in state.store.list_jobs().await? {
        for task in state.store.list_tasks_by_job(job.id).await? {
            if matches!(task.state, TaskState::Pending) {
                tasks.push(task);
            }
        }
    }

    if let Some(mut task) = tasks.into_iter().next() {
        if let Some(job) = state.store.get_job(task.job_id).await? {
            task.state = TaskState::Assigned { node_id };
            state.store.put_task(&task).await?;
            return Ok(Json(Some(Assignment {
                task_id: task.id,
                job_id: task.job_id,
                command: job.spec.command.clone(),
            })));
        }
    }
    Ok(Json(None))
}

async fn update_task_status(
    State(state): State<AppState>,
    Json(upd): Json<TaskStatusUpdate>
) -> Result<StatusCode, ApiError> {
    state.store.update_task_status(&upd).await?;
    Ok(StatusCode::NO_CONTENT)
}