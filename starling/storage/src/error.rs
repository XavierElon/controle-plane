use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("db error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
}

pub type StorageResult<T> = Result<T, StorageError>;