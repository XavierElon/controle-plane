use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde::Serialize;
use storage::error::StorageError;

#[derive(Debug)]
pub enum ApiError {
    NotFound,
    BadRequest(String),
    Storage(StorageError),
}

impl From<StorageError> for ApiError {
    fn from(e: StorageError) -> Self { ApiError::Storage(e) }
}

#[derive(Serialize)]
struct ErrorBody { message: String }

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::NotFound =>
                (StatusCode::NOT_FOUND, Json(ErrorBody { message: "not found".into() })).into_response(),
            ApiError::BadRequest(m) =>
                (StatusCode::BAD_REQUEST, Json(ErrorBody { message: m })).into_response(),
            ApiError::Storage(e) =>
                (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorBody { message: e.to_string() })).into_response()
        }
    }
}