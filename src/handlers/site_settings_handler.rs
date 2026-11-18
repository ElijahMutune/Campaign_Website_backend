use axum::{Json, extract::State, http::StatusCode};

use uuid::Uuid;

use serde::Deserialize;

use crate::{
    handlers::admin_handler::AdminApiError,
    models::site_settings::{PublicSiteSettings, SiteSettings},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct UpdateSiteSettingsRequest {
    pub site_name: Option<String>,
    pub site_description: Option<String>,
    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,
    pub contact_phone: Option<String>,
    pub contact_email: Option<String>,
    pub office_address: Option<String>,
}

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

fn valid_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value.as_bytes()[1..]
            .iter()
            .all(|byte| byte.is_ascii_hexdigit())
}

// Retrieve current website settings.
pub async fn get_site_settings(
    State(state): State<AppState>,
) -> Result<Json<SiteSettings>, (StatusCode, Json<AdminApiError>)> {
    let settings = sqlx::query_as::<_, SiteSettings>(
        r#"
        SELECT
            id,
            site_name,
            site_description,
            primary_color,
            secondary_color,
            contact_phone,
            contact_email,
            office_address,
            created_at,
            updated_at
        FROM site_settings
        ORDER BY updated_at DESC, created_at DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Settings retrieval error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve website settings.",
        )
    })?;

    match settings {
        Some(settings) => Ok(Json(settings)),
        None => Err(api_error(
            StatusCode::NOT_FOUND,
            "Website settings have not been configured yet.",
        )),
    }
}

// Create or update website settings.
pub async fn update_site_settings(
    State(state): State<AppState>,
    Json(payload): Json<UpdateSiteSettingsRequest>,
) -> Result<Json<SiteSettings>, (StatusCode, Json<AdminApiError>)> {
    let site_name = clean_optional(payload.site_name);
    let site_description = clean_optional(payload.site_description);
    let primary_color = clean_optional(payload.primary_color);
    let secondary_color = clean_optional(payload.secondary_color);
    let contact_phone = clean_optional(payload.contact_phone);
    let contact_email = clean_optional(payload.contact_email);
    let office_address = clean_optional(payload.office_address);

    // Validate field lengths.
    if site_name.as_ref().is_some_and(|v| v.chars().count() > 255)
        || site_description
            .as_ref()
            .is_some_and(|v| v.chars().count() > 10000)
        || contact_phone
            .as_ref()
            .is_some_and(|v| v.chars().count() > 50)
        || contact_email
            .as_ref()
            .is_some_and(|v| v.chars().count() > 255)
        || office_address
            .as_ref()
            .is_some_and(|v| v.chars().count() > 2000)
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "One or more settings exceed the permitted length.",
        ));
    }

    // Validate hexadecimal colours.
    if primary_color.as_ref().is_some_and(|v| !valid_color(v))
        || secondary_color.as_ref().is_some_and(|v| !valid_color(v))
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Colours must use hexadecimal format, for example #2563EB.",
        ));
    }

    // Basic contact email validation.
    if contact_email
        .as_ref()
        .is_some_and(|email| match email.split_once('@') {
            Some((local, domain)) => {
                local.is_empty()
                    || !domain.contains('.')
                    || domain.starts_with('.')
                    || domain.ends_with('.')
            }
            None => true,
        })
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Please provide a valid contact email address.",
        ));
    }

    // Protect against concurrent settings creation requests.
    let mut tx = state.db.begin().await.map_err(|error| {
        eprintln!("Settings transaction error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to save website settings.",
        )
    })?;

    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(2026111102_i64)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            eprintln!("Settings lock error: {error}");

            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to save website settings.",
            )
        })?;

    // Find the existing settings record, if one exists.
    let existing_id: Option<Uuid> = sqlx::query_scalar(
        r#"
        SELECT id
        FROM site_settings
        ORDER BY updated_at DESC, created_at DESC
        LIMIT 1
        FOR UPDATE
        "#,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| {
        eprintln!("Settings lookup error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to save website settings.",
        )
    })?;

    let settings = match existing_id {
        Some(id) => {
            // Update the existing record.
            sqlx::query_as::<_, SiteSettings>(
                r#"
                UPDATE site_settings
                SET
                    site_name = $2,
                    site_description = $3,
                    primary_color = $4,
                    secondary_color = $5,
                    contact_phone = $6,
                    contact_email = $7,
                    office_address = $8,
                    updated_at = NOW()
                WHERE id = $1
                RETURNING
                    id,
                    site_name,
                    site_description,
                    primary_color,
                    secondary_color,
                    contact_phone,
                    contact_email,
                    office_address,
                    created_at,
                    updated_at
                "#,
            )
            .bind(id)
            .bind(&site_name)
            .bind(&site_description)
            .bind(&primary_color)
            .bind(&secondary_color)
            .bind(&contact_phone)
            .bind(&contact_email)
            .bind(&office_address)
            .fetch_one(&mut *tx)
            .await
        }

        None => {
            // Create the first settings record.
            sqlx::query_as::<_, SiteSettings>(
                r#"
                INSERT INTO site_settings (
                    site_name,
                    site_description,
                    primary_color,
                    secondary_color,
                    contact_phone,
                    contact_email,
                    office_address
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                RETURNING
                    id,
                    site_name,
                    site_description,
                    primary_color,
                    secondary_color,
                    contact_phone,
                    contact_email,
                    office_address,
                    created_at,
                    updated_at
                "#,
            )
            .bind(&site_name)
            .bind(&site_description)
            .bind(&primary_color)
            .bind(&secondary_color)
            .bind(&contact_phone)
            .bind(&contact_email)
            .bind(&office_address)
            .fetch_one(&mut *tx)
            .await
        }
    }
    .map_err(|error| {
        eprintln!("Settings save error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to save website settings.",
        )
    })?;

    tx.commit().await.map_err(|error| {
        eprintln!("Settings commit error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to save website settings.",
        )
    })?;

    Ok(Json(settings))
}

// PUBLIC: RETRIEVE WEBSITE CONFIGURATION
pub async fn get_public_site_settings(
    State(state): State<AppState>,
) -> Result<Json<PublicSiteSettings>, (StatusCode, Json<AdminApiError>)> {
    let settings = sqlx::query_as::<_, PublicSiteSettings>(
        r#"
        SELECT
            site_name,
            site_description,
            primary_color,
            secondary_color,
            contact_phone,
            contact_email,
            office_address
        FROM site_settings
        ORDER BY updated_at DESC, created_at DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Public settings retrieval error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve website configuration.",
        )
    })?
    .ok_or_else(|| {
        api_error(
            StatusCode::NOT_FOUND,
            "Website configuration has not been configured yet.",
        )
    })?;

    Ok(Json(settings))
}
