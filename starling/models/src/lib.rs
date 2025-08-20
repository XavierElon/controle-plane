use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NodeRegistration {
    pub hostname: String,
    pub labels: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Node {
    #[schema(value_type = String, example = "123e4567-e89b-12d3-a456-426614174000")]
    pub id: Uuid,
    pub hostname: String,
    pub labels: Vec<(String, String)>,
    pub last_heartbeat: OffsetDateTime,
    pub lease_ttl_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct JobSpec {
    pub name: String,
    pub replicas: u32,
    pub command: Vec<String>,
    pub constraints: Vec<(String, String)>,
    pub priority: u8
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Job {
    pub id: Uuid,
    pub spec: JobSpec,
    pub created_at: OffsetDateTime
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum TaskState {
    Pending,
    Assigned {
        #[schema(value_type = String, example = "123e4567-e89b-12d3-a456-426614174000")]
        node_id: Uuid
    },
    Running {
        #[schema(value_type = String, example = "123e4567-e89b-12d3-a456-426614174000")]
        node_id: Uuid, 
        started_at: OffsetDateTime
    },
    Succeeded { finished_at: OffsetDateTime},
    Failed { finished_at: OffsetDateTime, attempts: u32, last_error: String}
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Task {
    #[schema(value_type = String, example = "123e4567-e89b-12d3-a456-426614174000")]
    pub id: Uuid,
    #[schema(value_type = String, example = "123e4567-e89b-12d3-a456-426614174000")]
    pub job_id: Uuid,
    pub state: TaskState,
    pub attempt: u32,
    pub priority: u8
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Assignment {
    #[schema(value_type = String, example = "123e4567-e89b-12d3-a456-426614174000")]
    pub task_id: Uuid,
    #[schema(value_type = String, example = "123e4567-e89b-12d3-a456-426614174000")]
    pub job_id: Uuid,
    pub command: Vec<String>
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TaskStatusUpdate {
    #[schema(value_type = String, example = "123e4567-e89b-12d3-a456-426614174000")]
    pub task_id: Uuid,
    pub state: TaskState
}