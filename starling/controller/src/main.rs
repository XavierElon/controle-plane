use axum::{
	extract::{Path, State},
	http::StatusCode,
	routing::{get, post},
	Json, Router,
};
use dashmap::DashMap;
use models::*;
use std::{net::SocketAddr, sync::Arc};
use time::OffsetDateTime;
use tokio::sync::mpsc;
use tokio::signal;
use tracing::{info, error};
use uuid::Uuid;
use dotenvy;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[derive(Clone)]
struct AppState {
    store: storage::DynStore,
    assignment_queues: Arc<DashMap<Uuid, mpsc::Sender<Assignment>>>,
    lease_ttl_secs: u64
}

#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        register_node,
        heartbeat,
        create_job,
        next_assignment,
        update_task_status
    ),
    components(schemas(
        models::NodeRegistration,
        models::Node,
        models::JobSpec,
        models::Job,
        models::TaskState,
        models::Task,
        models::Assignment,
        models::TaskStatusUpdate
    )),
    tags(
        (name = "controller", description = "Controller API")
    ),
    external_docs(
        url = "https://docs.rs/utoipa",
        description = "utoipa documentation"
    )
)]
struct ApiDoc;

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
        .merge(SwaggerUi::new("/swagger-ui").url("/openapi.json", ApiDoc::openapi()))
        .with_state(app_state);

    let addr: SocketAddr = "0.0.0.0:8090".parse().unwrap();
    info!("Starting controller on {}", addr);
    
    let server = axum::Server::bind(&addr)
        .serve(app.into_make_service());
    
    let graceful = server.with_graceful_shutdown(shutdown_signal());
    
    if let Err(e) = graceful.await {
        error!("Server error: {}", e);
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    
    info!("Shutdown signal received, starting graceful shutdown");
}

// async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
//     Json(ApiDoc::openapi())
// }

#[utoipa::path(
    post,
    path = "/v1/nodes/register",
    request_body = NodeRegistration,
    responses(
        (status = 200, description = "Node registered", body = Node)
    )
)]
async fn register_node(
    State(state): State<AppState>,
    Json(req): Json<NodeRegistration>) -> Result<Json<Node>, StatusCode> {
        let node = Node {
            id: Uuid::new_v4(),
            hostname: req.hostname,
            labels: req.labels,
            last_heartbeat: OffsetDateTime::now_utc(),
            lease_ttl_secs: state.lease_ttl_secs,
        };
        
        state.store.put_node(&node).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let (tx, _rx) = mpsc::channel::<Assignment>(128);
        state.assignment_queues.insert(node.id, tx);
        Ok(Json(node))
}

#[utoipa::path(
    post,
    path = "/v1/nodes/:id/heartbeat",
    params(
        ("id" = Uuid, Path, description = "Node ID")
    ),
    responses(
        (status = 204, description = "Heartbeat updated")
    )
)]
async fn heartbeat(
    State(state): State<AppState>,
    Path(id): Path<Uuid>) -> Result<StatusCode, StatusCode> {
        let Some(mut node) = state.store.get_node(id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? else {
            return Err(StatusCode::NOT_FOUND);
        };
        node.last_heartbeat = OffsetDateTime::now_utc(); 
        state.store.put_node(&node).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(StatusCode::NO_CONTENT)
}    

#[utoipa::path(
    post,
    path = "/v1/jobs",
    request_body = JobSpec,
    responses(
        (status = 200, description = "Job created", body = Job)
    )
)]
async fn create_job(State(state): State<AppState>, Json(spec): Json<JobSpec>) -> Result<Json<Job>, StatusCode> {
    let job = Job { id: Uuid::new_v4(), spec, created_at: OffsetDateTime::now_utc() };
    info!("Attempting to insert job: {:?}", job);

    if let Err(e) = state.store.put_job(&job).await {
        error!("Failed to insert job: {:?}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    // Naive: create N pending tasks and store them
    for _ in 0..job.spec.replicas {
        let t = Task {
            id: Uuid::new_v4(),
            job_id: job.id,
            state: TaskState::Pending,
            attempt: 0,
            priority: job.spec.priority,
        };
        if let Err(e) = state.store.put_task(&t).await {
            error!("Failed to insert task: {:?}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    info!("Successfully inserted job and tasks for job_id={}", job.id);
    Ok(Json(job))
}

#[utoipa::path(
    get,
    path = "/v1/agents/:id/next",
    params(
        ("id" = Uuid, Path, description = "Agent ID")
    ),
    responses(
        (status = 200, description = "Next assignment", body = Option<Assignment>)
    )
)]
async fn next_assignment(
    State(state): State<AppState>,
    Path(node_id): Path<Uuid>,
) -> Result<Json<Option<Assignment>>, StatusCode> {
    // Gather all pending tasks across all jobs
    let mut tasks = Vec::new();
    for job in state.store.list_jobs().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        for task in state.store.list_tasks_by_job(job.id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
            if matches!(task.state, TaskState::Pending) {
                tasks.push(task);
            }
        }
    }

    if let Some(mut task) = tasks.into_iter().next() {
        if let Some(job) = state
            .store
            .get_job(task.job_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        {
            task.state = TaskState::Assigned { node_id };
            state
                .store
                .put_task(&task)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            return Ok(Json(Some(Assignment {
                task_id: task.id,
                job_id: task.job_id,
                command: job.spec.command.clone(),
            })));
        }
    }
    Ok(Json(None))
}

#[utoipa::path(
    post,
    path = "/v1/tasks/status",
    request_body = TaskStatusUpdate,
    responses(
        (status = 204, description = "Task status updated")
    )
)]
async fn update_task_status(
    State(state): State<AppState>,
    Json(upd): Json<TaskStatusUpdate>
) -> Result<StatusCode, StatusCode> {
    state.store.update_task_status(&upd).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

