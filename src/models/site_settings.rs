use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, FromRow, Serialize)]
pub struct SiteSettings {
    pub id: Uuid,

    pub site_name: Option<String>,
    pub site_description: Option<String>,

    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,

    pub contact_phone: Option<String>,
    pub contact_email: Option<String>,
    pub office_address: Option<String>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct PublicSiteSettings {
    pub site_name: Option<String>,
    pub site_description: Option<String>,
    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,
    pub contact_phone: Option<String>,
    pub contact_email: Option<String>,
    pub office_address: Option<String>,
}
