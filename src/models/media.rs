use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, FromRow, Serialize)]
pub struct MediaRecord {
    pub id: Uuid,
    pub original_filename: String,
    pub mime_type: String,
    pub file_size: Option<i64>,
    pub media_type: String,
    pub alt_text: Option<String>,
    pub caption: Option<String>,
    pub uploaded_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}
