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
use tracing::info;
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    store: storage::DynStore,
    assignment_queues: Arc<DashMap<Uuid, mpsc::Sender<Assignment>>>,
    lease_ttl_secs: u64
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .pretty()
        .init();

    	// For now, use an in-memory store placeholder. Replace with sled-backed impl later.
    let store = Arc::new(InMemoryStore::default()) as storage::DynStore;
    let app_state = AppState {
        store,
        assignment_queues: Arc::new(DashMap::new()),
        lease_ttl_secs: 30
    };

    let app = Router::new()
        .route("/v1/nodes/register", post(register_node))
        .route("/v1/nodes/:id/heartbeat", post(heartbeat))
        .route("/v1/jobs", post(create_job))
        .route("/v1/agents/:id/next", get(next_assignment))
        .route("/v1/tasks/status", post(update_task_status))
        .with_state(app_state);

		let addr: SocketAddr = "0.0.0.0:8090".parse().unwrap();		info!("Starting controller on {}", addr);
		let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
		axum::serve(listener, app).await.unwrap();
}

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

async fn create_job(State(state): State<AppState>, Json(spec): Json<JobSpec>) -> Result<Json<Job>, StatusCode> {
    let job = Job { id: Uuid::new_v4(), spec, created_at: OffsetDateTime::now_utc() };
    state.store.put_job(&job).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Naive: create N pending tasks and store them
    for _ in 0..job.spec.replicas {
        let t = Task {``
            id: Uuid::new_v4(),
            job_id: job.id,
            state: TaskState::Pending,
            attempt: 0,
            priority:job.spec.priority,
        };
        state.store.put_task(&t).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    Ok(Json(job))
}

async fn next_assignment(
	State(state): State<AppState>,
	Path(node_id): Path<Uuid>,
) -> Result<Json<Option<Assignment>>, StatusCode> {
	let tasks = state
		.store
		.list_tasks_by_job_filter_pending()
		.await
		.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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

async fn update_task_status(
    State(state): State<AppState>,
    Json(upd): Json<TaskStatusUpdate>
) -> Result<StatusCode, StatusCode> {
    state.store.update_task_status(&upd).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}


// -------------------- Temporary in-memory store (for bootstrapping) --------------------

#[derive(Default, Clone)]
struct InMemoryStore {
	inner: Arc<dashmap::DashMap<Uuid, Node>>,
	jobs: Arc<dashmap::DashMap<Uuid, Job>>,
	tasks: Arc<dashmap::DashMap<Uuid, Task>>,
}

impl InMemoryStore {
    fn tasks_for_job(&self, job: Uuid) -> Vec<Task> {
        self.tasks.iter().filter(|e| e.job_id == job).map(|e| e.clone()).collect()
    }
}

#[axum::async_trait]
impl storage::Store for InMemoryStore {
	async fn put_node(&self, node: &Node) -> anyhow::Result<()> {
		self.inner.insert(node.id, node.clone());
		Ok(())
	}
	async fn get_node(&self, id: Uuid) -> anyhow::Result<Option<Node>> {
		Ok(self.inner.get(&id).map(|v| v.clone()))
	}
	async fn list_nodes(&self) -> anyhow::Result<Vec<Node>> {
		Ok(self.inner.iter().map(|n| n.clone()).collect())
	}
	async fn put_job(&self, job: &Job) -> anyhow::Result<()> {
		self.jobs.insert(job.id, job.clone());
		Ok(())
	}
	async fn get_job(&self, id: Uuid) -> anyhow::Result<Option<Job>> {
		Ok(self.jobs.get(&id).map(|v| v.clone()))
	}
	async fn list_jobs(&self) -> anyhow::Result<Vec<Job>> {
		Ok(self.jobs.iter().map(|j| j.clone()).collect())
	}
	async fn put_task(&self, task: &Task) -> anyhow::Result<()> {
		self.tasks.insert(task.id, task.clone());
		Ok(())
	}
	async fn get_task(&self, id: Uuid) -> anyhow::Result<Option<Task>> {
		Ok(self.tasks.get(&id).map(|v| v.clone()))
	}
	async fn list_tasks_by_job(&self, job: Uuid) -> anyhow::Result<Vec<Task>> {
		Ok(self.tasks_for_job(job))
	}
	async fn update_task_status(&self, upd: &TaskStatusUpdate) -> anyhow::Result<()> {
		if let Some(mut t) = self.tasks.get_mut(&upd.task_id) {
			*t = Task { state: upd.state.clone(), ..t.clone() };
		}
		Ok(())
	}
}

trait StoreExtra {
	fn list_tasks_by_job_filter_pending(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Vec<Task>>> + Send>>;
}

impl StoreExtra for storage::DynStore {
	fn list_tasks_by_job_filter_pending(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Vec<Task>>> + Send>> {
		let s = self.clone();
		Box::pin(async move {
			// naive: scan all jobs then tasks
			let mut out = Vec::new();
			for job in s.list_jobs().await? {
				for t in s.list_tasks_by_job(job.id).await? {
					if matches!(t.state, TaskState::Pending) {
						out.push(t);
					}
				}
			}
			Ok(out)
		})
	}
}