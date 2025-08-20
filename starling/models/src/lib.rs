use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRegistration {
    pub hostname: String,
    pub labels: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: Uuid,
    pub hostname: String,
    pub labels: Vec<(String, String)>,
    pub last_heartbeat: OffsetDateTime,
    pub lease_ttl_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSpec {
    pub name: String,
    pub replicas: u32,
    pub command: Vec<String>,
    pub constraints: Vec<(String, String)>,
    pub priority: u8
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: Uuid,
    pub spec: JobSpec,
    pub created_at: OffsetDateTime
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskState {
    Pending,
    Assigned {node_id: Uuid},
    Running {node_id: Uuid, started_at: OffsetDateTime},
    Succeeded { finished_at: OffsetDateTime},
    Failed { finished_at: OffsetDateTime, attempts: u32, last_error: String}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub job_id: Uuid,
    pub state: TaskState,
    pub attempt: u32,
    pub priority: u8
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assignment {
    pub task_id: Uuid,
    pub job_id: Uuid,
    pub command: Vec<String>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusUpdate {
    pub task_id: Uuid,
    pub state: TaskState
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;
    use uuid::Uuid;

    #[test]
    fn node_roundtrip_json() {
        let node = Node {
            id: Uuid::new_v4(),
            hostname: "test-host".to_string(),
            labels: vec![("env".to_string(), "test".to_string())],
            last_heartbeat: OffsetDateTime::now_utc(),
            lease_ttl_secs: 30,
        };
        let j = serde_json::to_string(&node).unwrap();
        let back: Node = serde_json::from_str(&j).unwrap();
        assert_eq!(node.hostname, back.hostname);
    }
}