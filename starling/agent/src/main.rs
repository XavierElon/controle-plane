use anyhow::Result;
use models::*;
use reqwest::Client;
use std::process::Stdio;
use std::time::Duration;
use tokio::{process::Command, time};
use tracing::{error, info};
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<()> {
	tracing_subscriber::fmt().with_env_filter("info").pretty().init();
	let controller = std::env::var("CONTROLLER_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
	let hostname = gethostname::gethostname().to_string_lossy().to_string();
	let client = Client::new();

	let reg = NodeRegistration { hostname, labels: vec![("zone".into(), "local".into())] };
	let node: Node = client.post(format!("{}/v1/nodes/register", controller)).json(&reg).send().await?.json().await?;
	info!("registered as {}", node.id);

	let node_id = node.id;
	let hb_url = format!("{}/v1/nodes/{}/heartbeat", controller, node_id);

	tokio::spawn({
		let client = client.clone();
		let hb_url = hb_url.clone();
		async move {
			let mut int = time::interval(Duration::from_secs(10));
			loop {
				int.tick().await;
				if let Err(e) = client.post(&hb_url).send().await {
					error!("heartbeat error: {e:?}");
				}
			}
		}
	});

	loop {
		let url = format!("{}/v1/agents/{}/next", controller, node_id);
		let resp = client.get(&url).send().await?;
		if resp.status().is_success() {
			let assignment: Option<Assignment> = resp.json().await?;
			if let Some(a) = assignment {
				info!("received assignment {}", a.task_id);
				run_task(&client, &controller, a).await?;
			} else {
				time::sleep(Duration::from_secs(2)).await;
			}
		} else {
			time::sleep(Duration::from_secs(5)).await;
		}
	}
}

async fn run_task(client: &Client, controller: &str, a: Assignment) -> Result<()> {
	let mut cmd = if a.command.is_empty() { "echo".to_string() } else { a.command[0].clone() };
	let args = if a.command.len() > 1 { a.command[1..].to_vec() } else { vec![] };

	let status_url = format!("{}/v1/tasks/status", controller);
	let start = TaskStatusUpdate {
		task_id: a.task_id,
		state: TaskState::Running { node_id: Uuid::nil(), started_at: time::OffsetDateTime::now_utc() },
	};
	let _ = client.post(&status_url).json(&start).send().await;

	let status = Command::new(&cmd).args(&args).stdout(Stdio::inherit()).stderr(Stdio::inherit()).status().await?;
	let end = if status.success() {
		TaskStatusUpdate {
			task_id: a.task_id,
			state: TaskState::Succeeded { finished_at: time::OffsetDateTime::now_utc() },
		}
	} else {
		TaskStatusUpdate {
			task_id: a.task_id,
			state: TaskState::Failed { finished_at: time::OffsetDateTime::now_utc(), attempts: 1, last_error: format!("exit: {}", status) },
		}
	};
	let _ = client.post(&status_url).json(&end).send().await;
	Ok(())
}