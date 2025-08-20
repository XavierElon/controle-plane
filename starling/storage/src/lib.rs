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

    async fn put_job(&self, job: &Job) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO jobs (id, spec, created_at)
            VALUES ($1, $2, $3)
            ON CONFLICT (id) DO UPDATE
            SET spec = EXCLUDED.spec,
                created_at = EXCLUDED.created_at
            "#,
            job.id,
            serde_json::to_value(&job.spec)?,
            job.created_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_job(&self, id: uuid::Uuid) -> Result<Option<Job>> {
        let rec = sqlx::query!(
            r#"
            SELECT id, spec, created_at
            FROM jobs WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = rec {
            Ok(Some(Job {
                id: row.id,
                spec: serde_json::from_value(row.spec)?,
                created_at: row.created_at,
            }))
        } else {
            Ok(None)
        }
    }

    async fn list_jobs(&self) -> Result<Vec<Job>> {
        let rows = sqlx::query!(
            r#"
            SELECT id, spec, created_at
            FROM jobs
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| Job {
                id: row.id,
                spec: serde_json::from_value(row.spec)?,
                created_at: row.created_at,
            })
            .collect())
    }

    // Task methods
    async fn put_task(&self, task: &Task) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO tasks (id, job_id, state, attempt, priority)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (id) DO UPDATE
            SET job_id = EXCLUDED.job_id,
                state = EXCLUDED.state,
                attempt = EXCLUDED.attempt,
                priority = EXCLUDED.priority
            "#,
            task.id,
            task.job_id,
            serde_json::to_value(&task.state)?,
            task.attempt as i32,
            task.priority as i16,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_task(&self, id: uuid::Uuid) -> Result<Option<Task>> {
        let rec = sqlx::query!(
            r#"
            SELECT id, job_id, state, attempt, priority
            FROM tasks WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = rec {
            Ok(Some(Task {
                id: row.id,
                job_id: row.job_id,
                state: serde_json::from_value(row.state)?,
                attempt: row.attempt as u32,
                priority: row.priority as u8,
            }))
        } else {
            Ok(None)
        }
    }

    async fn list_tasks_by_job(&self, job: uuid::Uuid) -> Result<Vec<Task>> {
        let rows = sqlx::query!(
            r#"
            SELECT id, job_id, state, attempt, priority
            FROM tasks WHERE job_id = $1
            "#,
            job
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| Task {
                id: row.id,
                job_id: row.job_id,
                state: serde_json::from_value(row.state).unwrap_or_default(),
                attempt: row.attempt as u32,
                priority: row.priority as u8,
            })
            .collect())
    }

    async fn update_task_status(&self, upd: &TaskStatusUpdate) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE tasks
            SET state = $2
            WHERE id = $1
            "#,
            upd.task_id,
            serde_json::to_value(&upd.state)?,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}