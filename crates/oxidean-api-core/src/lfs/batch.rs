//! Git LFS Batch API JSON types (basic transfer only — D-LFS-07).

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct BatchRequest {
    pub operation: String,
    #[serde(default)]
    pub transfers: Vec<String>,
    pub objects: Vec<BatchObjectIn>,
}

#[derive(Debug, Deserialize)]
pub struct BatchObjectIn {
    pub oid: String,
    pub size: i64,
}

#[derive(Debug, Serialize)]
pub struct BatchResponse {
    pub transfer: String,
    pub objects: Vec<BatchObjectOut>,
}

#[derive(Debug, Serialize)]
pub struct BatchObjectOut {
    pub oid: String,
    pub size: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actions: Option<BatchActions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<LfsObjectError>,
}

#[derive(Debug, Serialize)]
pub struct BatchActions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upload: Option<BatchAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download: Option<BatchAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify: Option<BatchAction>,
}

#[derive(Debug, Serialize)]
pub struct BatchAction {
    pub href: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct LfsObjectError {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct LfsErrorBody {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}
