use sqlx::PgPool;
use dotenvy::dotenv;
use std::env;

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    dotenv().ok();
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PgPool::connect(&database_url).await?;

    // Only run CREATE TABLE statements here!
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS nodes (
            id UUID PRIMARY KEY,
            hostname TEXT NOT NULL,
            labels JSONB NOT NULL,
            last_heartbeat TIMESTAMPTZ NOT NULL,
            lease_ttl_secs BIGINT NOT NULL
        );
        "#
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS jobs (
            id UUID PRIMARY KEY,
            spec JSONB NOT NULL,
            created_at TIMESTAMPTZ NOT NULL
        );
        "#
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS tasks (
            id UUID PRIMARY KEY,
            job_id UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
            state JSONB NOT NULL,
            attempt INTEGER NOT NULL,
            priority SMALLINT NOT NULL
        );
        "#
    )
    .execute(&pool)
    .await?;

    println!("Migration complete!");
    Ok(())
}