use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    handlers::admin_handler::AdminApiError, models::contact_message::ContactMessage,
    state::AppState,
};

fn api_error(status: StatusCode, message: &str) -> (StatusCode, Json<AdminApiError>) {
    (
        status,
        Json(AdminApiError {
            success: false,
            message: message.to_string(),
        }),
    )
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn valid_email(email: &str) -> bool {
    match email.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && !domain.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !email.chars().any(char::is_whitespace)
        }
        None => false,
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateContactMessageRequest {
    pub full_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub subject: Option<String>,
    pub message: String,

    // Honeypot: ordinary visitors leave this empty.
    // It is deliberately not saved in the database.
    pub website: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateContactMessageRequest {
    pub status: String,
    pub admin_notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ContactMessageQuery {
    pub status: Option<String>,
}

// PUBLIC: SUBMIT CONTACT MESSAGE
pub async fn create_contact_message(
    State(state): State<AppState>,
    Json(payload): Json<CreateContactMessageRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<AdminApiError>)> {
    // Silently accept honeypot submissions without storing them.
    // This avoids revealing the anti-spam field to automated bots.
    if clean_optional(payload.website).is_some() {
        return Ok((
            StatusCode::CREATED,
            Json(serde_json::json!({
                "success": true,
                "message": "Your message has been received."
            })),
        ));
    }

    let full_name = payload.full_name.trim().to_string();
    let email = clean_optional(payload.email);
    let phone = clean_optional(payload.phone);
    let subject = clean_optional(payload.subject);
    let message = payload.message.trim().to_string();

    if full_name.is_empty() || full_name.chars().count() > 150 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Full name is required and must not exceed 150 characters.",
        ));
    }

    if email.is_none() && phone.is_none() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Provide at least one contact method: email or phone.",
        ));
    }

    if email
        .as_ref()
        .is_some_and(|value| value.chars().count() > 255 || !valid_email(value))
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Please provide a valid email address.",
        ));
    }

    if phone
        .as_ref()
        .is_some_and(|value| value.chars().count() > 50)
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Phone number must not exceed 50 characters.",
        ));
    }

    if subject
        .as_ref()
        .is_some_and(|value| value.chars().count() > 255)
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Subject must not exceed 255 characters.",
        ));
    }

    if message.is_empty() || message.chars().count() > 10_000 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Message is required and must not exceed 10000 characters.",
        ));
    }

    let message_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO contact_messages (
            full_name,
            email,
            phone,
            subject,
            message,
            status
        )
        VALUES ($1, $2, $3, $4, $5, 'unread')
        RETURNING id
        "#,
    )
    .bind(&full_name)
    .bind(&email)
    .bind(&phone)
    .bind(&subject)
    .bind(&message)
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Contact message submission error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to submit your message.",
        )
    })?;

    // Return only a reference and acknowledgement, not submitted personal data.
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "success": true,
            "message": "Your message has been received.",
            "reference": message_id
        })),
    ))
}

// ADMIN: LIST CONTACT MESSAGES
// Optional filter: ?status=unread, read, or archived
pub async fn list_contact_messages(
    State(state): State<AppState>,
    Query(query): Query<ContactMessageQuery>,
) -> Result<Json<Vec<ContactMessage>>, (StatusCode, Json<AdminApiError>)> {
    let status = clean_optional(query.status);

    if status
        .as_ref()
        .is_some_and(|value| !matches!(value.as_str(), "unread" | "read" | "archived"))
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Status filter must be unread, read, or archived.",
        ));
    }

    let messages = sqlx::query_as::<_, ContactMessage>(
        r#"
        SELECT *
        FROM contact_messages
        WHERE ($1::text IS NULL OR status = $1)
        ORDER BY
            CASE WHEN status = 'unread' THEN 0 ELSE 1 END,
            created_at DESC
        "#,
    )
    .bind(&status)
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Contact message listing error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve contact messages.",
        )
    })?;

    Ok(Json(messages))
}

// ADMIN: GET ONE CONTACT MESSAGE
pub async fn get_contact_message(
    State(state): State<AppState>,
    Path(message_id): Path<Uuid>,
) -> Result<Json<ContactMessage>, (StatusCode, Json<AdminApiError>)> {
    let message =
        sqlx::query_as::<_, ContactMessage>("SELECT * FROM contact_messages WHERE id = $1")
            .bind(message_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|error| {
                eprintln!("Contact message retrieval error: {error}");
                api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Unable to retrieve contact message.",
                )
            })?
            .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Contact message not found."))?;

    Ok(Json(message))
}

// ADMIN: UPDATE MESSAGE STATUS AND PRIVATE NOTES
pub async fn update_contact_message(
    State(state): State<AppState>,
    Path(message_id): Path<Uuid>,
    Json(payload): Json<UpdateContactMessageRequest>,
) -> Result<Json<ContactMessage>, (StatusCode, Json<AdminApiError>)> {
    let status = payload.status.trim().to_lowercase();
    let admin_notes = clean_optional(payload.admin_notes);

    if !matches!(status.as_str(), "unread" | "read" | "archived") {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Status must be unread, read, or archived.",
        ));
    }

    if admin_notes
        .as_ref()
        .is_some_and(|value| value.chars().count() > 10_000)
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Administrator notes must not exceed 10000 characters.",
        ));
    }

    let message = sqlx::query_as::<_, ContactMessage>(
        r#"
        UPDATE contact_messages
        SET
            status = $2,
            admin_notes = $3,
            updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(message_id)
    .bind(&status)
    .bind(&admin_notes)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Contact message update error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update contact message.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Contact message not found."))?;

    Ok(Json(message))
}

// ADMIN: DELETE CONTACT MESSAGE
pub async fn delete_contact_message(
    State(state): State<AppState>,
    Path(message_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<AdminApiError>)> {
    let result = sqlx::query("DELETE FROM contact_messages WHERE id = $1")
        .bind(message_id)
        .execute(&state.db)
        .await
        .map_err(|error| {
            eprintln!("Contact message deletion error: {error}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to delete contact message.",
            )
        })?;

    if result.rows_affected() == 0 {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            "Contact message not found.",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
