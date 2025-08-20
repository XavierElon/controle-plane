use storage::{SqlxStore, create_pool, Store};
use uuid::Uuid;
use time::OffsetDateTime;
use models::Node;

async fn setup() -> SqlxStore {
    let url = std::env::var("TEST_DATABASE_URL").expect("set TEST_DATABASE_URL");
    let pool = create_pool(&url).await;
    SqlxStore::new(pool)
}

#[tokio::test]
async fn node_crud() {
    let store = setup().await;
    let node = Node {
        id: Uuid::new_v4(),
        hostname: "test-node".to_string(),
        labels: vec![("env".to_string(), "test".to_string())],
        last_heartbeat: OffsetDateTime::now_utc(),
        lease_ttl_secs: 30,
    };
    store.put_node(&node).await.unwrap();
    let back = store.get_node(node.id).await.unwrap().unwrap();
    assert_eq!(node.hostname, back.hostname);
}