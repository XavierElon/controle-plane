use anyhow::Result;
use async_trait:: async_trait;
use models::{Job, Node, Task, TaskStatusUpdate};
use std::sync::Arc;
use sqlx::{PgPool, postgres::PgPoolOptions};
// use sqlx::types::Json; 


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
impl Store for SqlxStore {
    async fn put_node(&self, node: &Node) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO nodes (id, hostname, labels, last_heartbeat, lease_ttl_secs)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (id) DO UPDATE
            SET hostname = EXCLUDED.hostname,
                labels = EXCLUDED.labels,
                last_heartbeat = EXCLUDED.last_heartbeat,
                lease_ttl_secs = EXCLUDED.lease_ttl_secs
            "#,
            node.id,
            node.hostname,
            serde_json::to_value(&node.labels)?,
            node.last_heartbeat,
            node.lease_ttl_secs as i64,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_node(&self, id: uuid::Uuid) -> Result<Option<Node>> {
        let rec = sqlx::query!(
            r#"
            SELECT id, hostname, labels, last_heartbeat, lease_ttl_secs
            FROM nodes WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = rec {
            Ok(Some(Node {
                id: row.id,
                hostname: row.hostname,
                labels: serde_json::from_value(row.labels)?,
                last_heartbeat: row.last_heartbeat,
                lease_ttl_secs: row.lease_ttl_secs as u64,
            }))
        } else {
            Ok(None)
        }
    }

    async fn list_nodes(&self) -> Result<Vec<Node>> {
        let rows = sqlx::query!(
            r#"
            SELECT id, hostname, labels, last_heartbeat, lease_ttl_secs
            FROM nodes
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| Node {
                id: row.id,
                hostname: row.hostname,
                labels: serde_json::from_value(row.labels).unwrap_or_default(),
                last_heartbeat: row.last_heartbeat,
                lease_ttl_secs: row.lease_ttl_secs as u64,
            })
            .collect())
    }
    async fn put_job(&self, _job: &Job) -> Result<()> {
        todo!()
    }
    async fn get_job(&self, _id: uuid::Uuid) -> Result<Option<Job>> {
        todo!()
    }
    async fn list_jobs(&self) -> Result<Vec<Job>> {
        todo!()
    }
    async fn put_task(&self, _task: &Task) -> Result<()> {
        todo!()
    }
    async fn get_task(&self, _id: uuid::Uuid) -> Result<Option<Task>> {
        todo!()
    }
    async fn list_tasks_by_job(&self, _job: uuid::Uuid) -> Result<Vec<Task>> {
        todo!()
    }
    async fn update_task_status(&self, _upd: &TaskStatusUpdate) -> Result<()> {
        todo!()
    }
}