use anyhow::Result;
use async_trait:: async_trait;
use models::{Job, Node, Task, TaskStatusUpdate};
use std::sync::Arc;
use sqlx::{PgPool, postgres::PgPoolOptions};

#[async_trait]
pub trait Store: Send + Sync {
    async fn put_node(&self, node: &Node) -> Result<()>;
    async fn get_node(&self, id: uuid::Uuid) -> Result<Option<Node>>;
    async fn list_nodes(&self) -> Result<Vec<Node>>;

    async fn put_job(&self, job: &Job) -> Result<()>;
    async fn get_job(&self, id: uuid::Uuid) -> Result<Option<Job>>;
    async fn list_jobs(&self) -> Result<Vec<Job>>;

    async fn put_task(&self, task: &Task) -> Result<()>;
	async fn get_task(&self, id: uuid::Uuid) -> Result<Option<Task>>;
	async fn list_tasks_by_job(&self, job: uuid::Uuid) -> Result<Vec<Task>>;

	async fn update_task_status(&self, upd: &TaskStatusUpdate) -> Result<()>;
}

pub type DynStore = Arc<dyn Store>;

pub async fn create_pool(database_url: &str) -> PgPool {
    PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
        .expect("Failed to create database pool")
}

pub struct SqlxStore {
    pool: PgPool,
}

impl SqlxStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
import Store for SqlxStore {
    async fn put_node(&self, node: &Node) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO nodes (id, name, last_seen)
            VALUES ($1, $2, $3)
            ON CONFLICT (id) DO UPDATE
            SET name = EXCLUDED.name, last_seen = EXCLUDED.last_seen
            "#,
            node.id,
            node.name,
            node.last_seen,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_node(&self, id: uuid::Uuid) -> Result<Option<Node>> {
        let rec = sqlx::query!(
            r#"
            SELECT id, name, last_seen FROM nodes WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(rec.map(|r| Node {
            id: r.id,
            name: r.name,
            last_seen: r.last_seen,
        }))
    }
}