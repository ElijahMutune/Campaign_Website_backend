use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    handlers::admin_handler::AdminApiError,
    models::talent_application::{
        PublicTalentShowcase, TalentApplication, TalentApplicationAttachment,
    },
    models::talent_category::TalentCategory,
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

fn validate_category_name(name: &str) -> Result<(), (StatusCode, Json<AdminApiError>)> {
    if name.is_empty() || name.chars().count() > 150 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Category name is required and must not exceed 150 characters.",
        ));
    }

    Ok(())
}

async fn validate_active_category(
    state: &AppState,
    category_id: Option<Uuid>,
) -> Result<(), (StatusCode, Json<AdminApiError>)> {
    let Some(category_id) = category_id else {
        return Ok(());
    };

    let exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM talent_categories
            WHERE id = $1 AND is_active = TRUE
        )
        "#,
    )
    .bind(category_id)
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Talent category validation error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to validate talent category.",
        )
    })?;

    if !exists {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Selected talent category does not exist or is inactive.",
        ));
    }

    Ok(())
}

async fn validate_private_attachment(
    state: &AppState,
    media_id: Uuid,
) -> Result<(), (StatusCode, Json<AdminApiError>)> {
    let exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM media
            WHERE id = $1
              AND media_type IN ('image', 'video')
              AND deleted_at IS NULL
        )
        "#,
    )
    .bind(media_id)
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Talent attachment validation error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to validate attachment.",
        )
    })?;

    if !exists {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Attachment must be an active image or video in the media library.",
        ));
    }

    Ok(())
}

async fn validate_public_image(
    state: &AppState,
    media_id: Option<Uuid>,
) -> Result<(), (StatusCode, Json<AdminApiError>)> {
    let Some(media_id) = media_id else {
        return Ok(());
    };

    let exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM media m
            WHERE m.id = $1
              AND m.media_type = 'image'
              AND m.deleted_at IS NULL
              AND NOT EXISTS (
                  SELECT 1
                  FROM talent_application_media tam
                  WHERE tam.media_id = m.id
              )
        )
        "#,
    )
    .bind(media_id)
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Public talent image validation error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to validate public showcase image.",
        )
    })?;

    if !exists {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Public showcase image must be an active image not assigned as a private application attachment.",
        ));
    }

    Ok(())
}

// -----------------------------------------------------------------------------
// REQUEST MODELS
// -----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateTalentCategoryRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTalentCategoryRequest {
    pub name: String,
    pub description: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateTalentApplicationRequest {
    pub full_name: String,
    pub age: Option<i32>,
    pub phone: String,
    pub email: Option<String>,
    pub location: Option<String>,
    pub category_id: Option<Uuid>,
    pub talent_description: String,
    pub achievements: Option<String>,
    pub support_needed: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTalentReviewRequest {
    pub status: String,
    pub admin_notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTalentPublicationRequest {
    pub is_public: bool,
    pub public_display_name: Option<String>,
    pub public_summary: Option<String>,
    pub public_media_id: Option<Uuid>,
}
#[derive(Debug, Deserialize)]
pub struct AddTalentAttachmentRequest {
    pub media_id: Uuid,
}
// -----------------------------------------------------------------------------
// TALENT CATEGORIES
// -----------------------------------------------------------------------------

// ADMIN: CREATE TALENT CATEGORY
pub async fn create_talent_category(
    State(state): State<AppState>,
    Json(payload): Json<CreateTalentCategoryRequest>,
) -> Result<(StatusCode, Json<TalentCategory>), (StatusCode, Json<AdminApiError>)> {
    let name = payload.name.trim().to_string();
    let description = clean_optional(payload.description);

    validate_category_name(&name)?;

    let category = sqlx::query_as::<_, TalentCategory>(
        r#"
        INSERT INTO talent_categories (name, description)
        VALUES ($1, $2)
        RETURNING id, name, description, is_active, created_at
        "#,
    )
    .bind(&name)
    .bind(&description)
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        if error.to_string().contains("talent_categories_name_key") {
            api_error(
                StatusCode::CONFLICT,
                "A talent category with this name already exists.",
            )
        } else {
            eprintln!("Talent category creation error: {error}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to create talent category.",
            )
        }
    })?;

    Ok((StatusCode::CREATED, Json(category)))
}

// ADMIN: LIST ALL CATEGORIES
pub async fn list_talent_categories(
    State(state): State<AppState>,
) -> Result<Json<Vec<TalentCategory>>, (StatusCode, Json<AdminApiError>)> {
    let categories = sqlx::query_as::<_, TalentCategory>(
        r#"
        SELECT id, name, description, is_active, created_at
        FROM talent_categories
        ORDER BY name ASC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Talent category listing error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve talent categories.",
        )
    })?;

    Ok(Json(categories))
}

// PUBLIC: LIST ACTIVE CATEGORIES
pub async fn list_public_talent_categories(
    State(state): State<AppState>,
) -> Result<Json<Vec<TalentCategory>>, (StatusCode, Json<AdminApiError>)> {
    let categories = sqlx::query_as::<_, TalentCategory>(
        r#"
        SELECT id, name, description, is_active, created_at
        FROM talent_categories
        WHERE is_active = TRUE
        ORDER BY name ASC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Public talent category listing error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve talent categories.",
        )
    })?;

    Ok(Json(categories))
}

// ADMIN: UPDATE CATEGORY
pub async fn update_talent_category(
    State(state): State<AppState>,
    Path(category_id): Path<Uuid>,
    Json(payload): Json<UpdateTalentCategoryRequest>,
) -> Result<Json<TalentCategory>, (StatusCode, Json<AdminApiError>)> {
    let name = payload.name.trim().to_string();
    let description = clean_optional(payload.description);

    validate_category_name(&name)?;

    let category = sqlx::query_as::<_, TalentCategory>(
        r#"
        UPDATE talent_categories
        SET
            name = $2,
            description = $3,
            is_active = $4
        WHERE id = $1
        RETURNING id, name, description, is_active, created_at
        "#,
    )
    .bind(category_id)
    .bind(&name)
    .bind(&description)
    .bind(payload.is_active)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        if error.to_string().contains("talent_categories_name_key") {
            api_error(
                StatusCode::CONFLICT,
                "Another talent category already uses this name.",
            )
        } else {
            eprintln!("Talent category update error: {error}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to update talent category.",
            )
        }
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Talent category not found."))?;

    Ok(Json(category))
}

// ADMIN: DELETE CATEGORY
// Existing applications are retained with category_id set to NULL.
pub async fn delete_talent_category(
    State(state): State<AppState>,
    Path(category_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<AdminApiError>)> {
    let result = sqlx::query("DELETE FROM talent_categories WHERE id = $1")
        .bind(category_id)
        .execute(&state.db)
        .await
        .map_err(|error| {
            eprintln!("Talent category deletion error: {error}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to delete talent category.",
            )
        })?;

    if result.rows_affected() == 0 {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            "Talent category not found.",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

// -----------------------------------------------------------------------------
// TALENT APPLICATIONS
// -----------------------------------------------------------------------------

// PUBLIC: SUBMIT TALENT APPLICATION
// Applicant details are private and are never returned by a public endpoint.
pub async fn create_talent_application(
    State(state): State<AppState>,
    Json(payload): Json<CreateTalentApplicationRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<AdminApiError>)> {
    let full_name = payload.full_name.trim().to_string();
    let phone = payload.phone.trim().to_string();
    let email = clean_optional(payload.email);
    let location = clean_optional(payload.location);
    let talent_description = payload.talent_description.trim().to_string();
    let achievements = clean_optional(payload.achievements);
    let support_needed = clean_optional(payload.support_needed);

    if full_name.is_empty() || full_name.chars().count() > 150 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Full name is required and must not exceed 150 characters.",
        ));
    }

    if phone.chars().count() < 5 || phone.chars().count() > 50 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "A valid contact phone number is required.",
        ));
    }

    if let Some(email) = &email {
        if email.chars().count() > 255 || !email.contains('@') {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "Please provide a valid email address.",
            ));
        }
    }

    if let Some(age) = payload.age {
        if !(1..=120).contains(&age) {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "Age must be between 1 and 120.",
            ));
        }
    }

    if location
        .as_ref()
        .is_some_and(|value| value.chars().count() > 255)
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Location must not exceed 255 characters.",
        ));
    }

    if talent_description.is_empty() || talent_description.chars().count() > 20_000 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Talent description is required and must not exceed 20000 characters.",
        ));
    }

    validate_active_category(&state, payload.category_id).await?;

    let (application_id, upload_token): (Uuid, Uuid) = sqlx::query_as(
        r#"
    INSERT INTO talent_applications (
        full_name,
        age,
        phone,
        email,
        location,
        category_id,
        talent_description,
        achievements,
        support_needed,
        status,
        is_public
    )
    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'new', FALSE)
    RETURNING id, attachment_upload_token
    "#,
    )
    .bind(&full_name)
    .bind(payload.age)
    .bind(&phone)
    .bind(&email)
    .bind(&location)
    .bind(payload.category_id)
    .bind(&talent_description)
    .bind(&achievements)
    .bind(&support_needed)
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Talent application submission error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to submit talent application.",
        )
    })?;

    // Deliberately return only the reference and acknowledgement.
    // Never echo personal information from a public submission.
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "success": true,
            "message": "Your talent application has been received for review.",
            "application_reference": application_id,
            "attachment_upload_token": upload_token
        })),
    ))
}

// ADMIN: LIST APPLICATIONS
pub async fn list_talent_applications(
    State(state): State<AppState>,
) -> Result<Json<Vec<TalentApplication>>, (StatusCode, Json<AdminApiError>)> {
    let applications = sqlx::query_as::<_, TalentApplication>(
        r#"
        SELECT *
        FROM talent_applications
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Talent application listing error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve talent applications.",
        )
    })?;

    Ok(Json(applications))
}

// ADMIN: GET ONE APPLICATION
pub async fn get_talent_application(
    State(state): State<AppState>,
    Path(application_id): Path<Uuid>,
) -> Result<Json<TalentApplication>, (StatusCode, Json<AdminApiError>)> {
    let application =
        sqlx::query_as::<_, TalentApplication>("SELECT * FROM talent_applications WHERE id = $1")
            .bind(application_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|error| {
                eprintln!("Talent application retrieval error: {error}");
                api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Unable to retrieve talent application.",
                )
            })?
            .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Talent application not found."))?;

    Ok(Json(application))
}

// ADMIN: REVIEW APPLICATION
pub async fn update_talent_review(
    State(state): State<AppState>,
    Path(application_id): Path<Uuid>,
    Json(payload): Json<UpdateTalentReviewRequest>,
) -> Result<Json<TalentApplication>, (StatusCode, Json<AdminApiError>)> {
    let status = payload.status.trim().to_lowercase();
    let admin_notes = clean_optional(payload.admin_notes);

    if !matches!(
        status.as_str(),
        "new" | "under_review" | "contacted" | "supported" | "rejected" | "archived"
    ) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Invalid application status.",
        ));
    }

    let application = sqlx::query_as::<_, TalentApplication>(
        r#"
        UPDATE talent_applications
SET
    status = $2,
    admin_notes = $3,
    is_public = CASE
        WHEN $2 = 'supported' THEN is_public
        ELSE FALSE
    END,
    public_display_name = CASE
        WHEN $2 = 'supported' THEN public_display_name
        ELSE NULL
    END,
    public_summary = CASE
        WHEN $2 = 'supported' THEN public_summary
        ELSE NULL
    END,
    public_media_id = CASE
        WHEN $2 = 'supported' THEN public_media_id
        ELSE NULL
    END,
    published_at = CASE
        WHEN $2 = 'supported' THEN published_at
        ELSE NULL
    END,
    updated_at = NOW()
WHERE id = $1
RETURNING *
        "#,
    )
    .bind(application_id)
    .bind(&status)
    .bind(&admin_notes)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Talent review update error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update talent application review.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Talent application not found."))?;

    Ok(Json(application))
}

// ADMIN: ATTACH EXISTING PRIVATE MEDIA TO AN APPLICATION
// Only admin users can perform this association.
// Media records and their files are not copied or duplicated.
pub async fn add_talent_application_attachment(
    State(state): State<AppState>,
    Path(application_id): Path<Uuid>,
    Json(payload): Json<AddTalentAttachmentRequest>,
) -> Result<(StatusCode, Json<TalentApplicationAttachment>), (StatusCode, Json<AdminApiError>)> {
    validate_private_attachment(&state, payload.media_id).await?;

    let application_exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM talent_applications WHERE id = $1)")
            .bind(application_id)
            .fetch_one(&state.db)
            .await
            .map_err(|error| {
                eprintln!("Talent application lookup error: {error}");
                api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Unable to validate talent application.",
                )
            })?;

    if !application_exists {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            "Talent application not found.",
        ));
    }

    let result = sqlx::query(
        r#"
        INSERT INTO talent_application_media (application_id, media_id)
        VALUES ($1, $2)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(application_id)
    .bind(payload.media_id)
    .execute(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Talent attachment association error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to attach media to application.",
        )
    })?;

    if result.rows_affected() == 0 {
        return Err(api_error(
            StatusCode::CONFLICT,
            "This attachment is already associated with the application.",
        ));
    }

    Ok((
        StatusCode::CREATED,
        Json(TalentApplicationAttachment {
            application_id,
            media_id: payload.media_id,
        }),
    ))
}

// ADMIN: LIST PRIVATE ATTACHMENT REFERENCES
pub async fn list_talent_application_attachments(
    State(state): State<AppState>,
    Path(application_id): Path<Uuid>,
) -> Result<Json<Vec<TalentApplicationAttachment>>, (StatusCode, Json<AdminApiError>)> {
    let attachments = sqlx::query_as::<_, TalentApplicationAttachment>(
        r#"
        SELECT application_id, media_id
        FROM talent_application_media
        WHERE application_id = $1
        ORDER BY media_id
        "#,
    )
    .bind(application_id)
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Talent attachment listing error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve application attachments.",
        )
    })?;

    Ok(Json(attachments))
}

// ADMIN: REMOVE PRIVATE ATTACHMENT ASSOCIATION
// The media record and physical file remain in the media library.
pub async fn remove_talent_application_attachment(
    State(state): State<AppState>,
    Path((application_id, media_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, Json<AdminApiError>)> {
    let result = sqlx::query(
        r#"
        DELETE FROM talent_application_media
        WHERE application_id = $1 AND media_id = $2
        "#,
    )
    .bind(application_id)
    .bind(media_id)
    .execute(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Talent attachment removal error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to remove application attachment.",
        )
    })?;

    if result.rows_affected() == 0 {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            "Application attachment not found.",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

// ADMIN: PUBLISH OR UNPUBLISH A SANITIZED SHOWCASE
pub async fn update_talent_publication(
    State(state): State<AppState>,
    Path(application_id): Path<Uuid>,
    Json(payload): Json<UpdateTalentPublicationRequest>,
) -> Result<Json<TalentApplication>, (StatusCode, Json<AdminApiError>)> {
    let display_name = clean_optional(payload.public_display_name);
    let summary = clean_optional(payload.public_summary);

    if payload.is_public {
        if display_name
            .as_ref()
            .is_none_or(|value| value.chars().count() > 150)
        {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "A public display name is required and must not exceed 150 characters.",
            ));
        }

        if summary
            .as_ref()
            .is_none_or(|value| value.chars().count() > 5000)
        {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "A public summary is required and must not exceed 5000 characters.",
            ));
        }

        validate_public_image(&state, payload.public_media_id).await?;

        let supported: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM talent_applications
                WHERE id = $1 AND status = 'supported'
            )
            "#,
        )
        .bind(application_id)
        .fetch_one(&state.db)
        .await
        .map_err(|error| {
            eprintln!("Talent publication status check error: {error}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to verify application approval.",
            )
        })?;

        if !supported {
            return Err(api_error(
                StatusCode::CONFLICT,
                "Only applications marked as supported can be published publicly.",
            ));
        }
    }

    let application = sqlx::query_as::<_, TalentApplication>(
        r#"
        UPDATE talent_applications
        SET
            is_public = $2,
            public_display_name = CASE WHEN $2 THEN $3 ELSE NULL END,
            public_summary = CASE WHEN $2 THEN $4 ELSE NULL END,
            public_media_id = CASE WHEN $2 THEN $5 ELSE NULL END,
            published_at = CASE
                WHEN $2 THEN COALESCE(published_at, NOW())
                ELSE NULL
            END,
            updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(application_id)
    .bind(payload.is_public)
    .bind(&display_name)
    .bind(&summary)
    .bind(payload.public_media_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Talent publication update error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update talent showcase publication.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Talent application not found."))?;

    Ok(Json(application))
}

// PUBLIC: LIST APPROVED SHOWCASE ENTRIES ONLY
// No applicant contact details, age, notes or private attachments are selected.
pub async fn list_public_talent_showcase(
    State(state): State<AppState>,
) -> Result<Json<Vec<PublicTalentShowcase>>, (StatusCode, Json<AdminApiError>)> {
    let entries = sqlx::query_as::<_, PublicTalentShowcase>(
        r#"
        SELECT
            id,
            public_display_name,
            public_summary,
            public_media_id,
            published_at
        FROM talent_applications
        WHERE is_public = TRUE
          AND published_at IS NOT NULL
          AND status = 'supported'
        ORDER BY published_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Public talent showcase listing error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve public talent showcase.",
        )
    })?;

    Ok(Json(entries))
}

// PUBLIC: GET ONE APPROVED SHOWCASE ENTRY
pub async fn get_public_talent_showcase(
    State(state): State<AppState>,
    Path(application_id): Path<Uuid>,
) -> Result<Json<PublicTalentShowcase>, (StatusCode, Json<AdminApiError>)> {
    let entry = sqlx::query_as::<_, PublicTalentShowcase>(
        r#"
        SELECT
            id,
            public_display_name,
            public_summary,
            public_media_id,
            published_at
        FROM talent_applications
        WHERE id = $1
          AND is_public = TRUE
          AND published_at IS NOT NULL
          AND status = 'supported'
        "#,
    )
    .bind(application_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Public talent showcase retrieval error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve public talent showcase entry.",
        )
    })?
    .ok_or_else(|| {
        api_error(
            StatusCode::NOT_FOUND,
            "Public talent showcase entry not found.",
        )
    })?;

    Ok(Json(entry))
}
