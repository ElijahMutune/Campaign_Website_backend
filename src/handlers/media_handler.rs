use axum::{
    Json,
    body::Body,
    extract::{Multipart, Path, State},
    http::{StatusCode, header},
    response::Response,
};

use image::{ImageFormat, ImageReader};
use std::{env, io::Cursor, path::PathBuf};

use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::{
    handlers::admin_handler::AdminApiError, models::media::MediaRecord,
    models::user::AuthenticatedUser, state::AppState,
};

const MAX_UPLOAD_BYTES: usize = 8 * 1024 * 1024;
const MAX_IMAGE_PIXELS: u64 = 40_000_000;
const MAX_IMAGE_DIMENSION: u32 = 12_000;

fn api_error(status: StatusCode, message: &str) -> (StatusCode, Json<AdminApiError>) {
    (
        status,
        Json(AdminApiError {
            success: false,
            message: message.to_string(),
        }),
    )
}

// ======================================================
// IMAGE VALIDATION
// ======================================================

// Verify actual image contents, not the filename or supplied MIME type.
fn validate_image(bytes: &[u8]) -> Result<(&'static str, &'static str), String> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| "Unable to identify image format".to_string())?;

    let format = reader
        .format()
        .ok_or_else(|| "Unknown image format".to_string())?;

    let (mime_type, extension) = match format {
        ImageFormat::Jpeg => ("image/jpeg", "jpg"),
        ImageFormat::Png => ("image/png", "png"),
        ImageFormat::WebP => ("image/webp", "webp"),
        _ => {
            return Err("Only JPEG, PNG and WebP images are accepted".to_string());
        }
    };

    let (width, height) = reader
        .into_dimensions()
        .map_err(|_| "Invalid image file".to_string())?;

    if width == 0
        || height == 0
        || width > MAX_IMAGE_DIMENSION
        || height > MAX_IMAGE_DIMENSION
        || (width as u64 * height as u64) > MAX_IMAGE_PIXELS
    {
        return Err("Image dimensions exceed the permitted limits".to_string());
    }

    // Fully decode the image to reject corrupt or incomplete files.
    image::load_from_memory_with_format(bytes, format)
        .map_err(|_| "Image file is corrupt or invalid".to_string())?;

    Ok((mime_type, extension))
}

// Never use the uploaded filename as the storage filename.
fn clean_original_filename(filename: &str) -> String {
    let basename = filename
        .rsplit(|character| character == '/' || character == '\\')
        .next()
        .unwrap_or("upload");

    let cleaned: String = basename
        .chars()
        .filter(|character| !character.is_control())
        .take(500)
        .collect();

    if cleaned.is_empty() {
        "upload".to_string()
    } else {
        cleaned
    }
}

// ======================================================
// UPLOAD MEDIA
// ======================================================

pub async fn upload_media(
    axum::extract::Extension(user): axum::extract::Extension<AuthenticatedUser>,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<MediaRecord>), (StatusCode, Json<AdminApiError>)> {
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut original_filename: Option<String> = None;
    let mut alt_text: Option<String> = None;
    let mut caption: Option<String> = None;

    // Read multipart fields.
    while let Some(field) = multipart.next_field().await.map_err(|error| {
        eprintln!("Multipart error: {error}");

        api_error(
            StatusCode::BAD_REQUEST,
            "Invalid or oversized upload request.",
        )
    })? {
        let field_name = field.name().unwrap_or("").to_string();

        match field_name.as_str() {
            "file" => {
                if file_bytes.is_some() {
                    return Err(api_error(
                        StatusCode::BAD_REQUEST,
                        "Only one image can be uploaded per request.",
                    ));
                }

                let filename = field.file_name().unwrap_or("upload").to_string();

                let bytes = field.bytes().await.map_err(|error| {
                    eprintln!("Image reading error: {error}");

                    api_error(StatusCode::BAD_REQUEST, "Unable to read uploaded image.")
                })?;

                if bytes.is_empty() {
                    return Err(api_error(
                        StatusCode::BAD_REQUEST,
                        "Uploaded file is empty.",
                    ));
                }

                if bytes.len() > MAX_UPLOAD_BYTES {
                    return Err(api_error(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        "Image must not exceed 8 MB.",
                    ));
                }

                file_bytes = Some(bytes.to_vec());
                original_filename = Some(filename);
            }

            "alt_text" => {
                let value = field
                    .text()
                    .await
                    .map_err(|_| api_error(StatusCode::BAD_REQUEST, "Invalid image alt text."))?;

                if value.chars().count() > 255 {
                    return Err(api_error(
                        StatusCode::BAD_REQUEST,
                        "Alt text must not exceed 255 characters.",
                    ));
                }

                alt_text = Some(value.trim().to_string()).filter(|value| !value.is_empty());
            }

            "caption" => {
                let value = field
                    .text()
                    .await
                    .map_err(|_| api_error(StatusCode::BAD_REQUEST, "Invalid image caption."))?;

                if value.chars().count() > 5000 {
                    return Err(api_error(
                        StatusCode::BAD_REQUEST,
                        "Caption must not exceed 5000 characters.",
                    ));
                }

                caption = Some(value.trim().to_string()).filter(|value| !value.is_empty());
            }

            _ => {
                return Err(api_error(
                    StatusCode::BAD_REQUEST,
                    "Unsupported upload field.",
                ));
            }
        }
    }

    let file_bytes = file_bytes
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "An image file is required."))?;

    let original_filename =
        clean_original_filename(original_filename.as_deref().unwrap_or("upload"));

    // Image decoding is CPU-intensive, so run it outside the async runtime.
    let validation_bytes = file_bytes.clone();

    let validation = tokio::task::spawn_blocking(move || validate_image(&validation_bytes))
        .await
        .map_err(|error| {
            eprintln!("Image validation task error: {error}");

            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to validate image.",
            )
        })?
        .map_err(|error| api_error(StatusCode::BAD_REQUEST, &error))?;

    let (mime_type, extension) = validation;

    // Obtain storage directory from environment.
    let storage_dir = env::var("MEDIA_STORAGE_DIR").unwrap_or_else(|_| "uploads/media".to_string());

    let storage_path = PathBuf::from(&storage_dir);

    tokio::fs::create_dir_all(&storage_path)
        .await
        .map_err(|error| {
            eprintln!("Storage directory error: {error}");

            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to prepare media storage.",
            )
        })?;

    // Generate a unique filename.
    let stored_filename = format!("{}.{}", Uuid::new_v4(), extension);

    let full_path = storage_path.join(&stored_filename);

    let file_path = full_path.to_string_lossy().to_string();

    // create_new prevents overwriting an existing file.
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&full_path)
        .await
        .map_err(|error| {
            eprintln!("Image storage error: {error}");

            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to create unique image file.",
            )
        })?;

    if let Err(error) = file.write_all(&file_bytes).await {
        eprintln!("Image writing error: {error}");

        drop(file);

        let _ = tokio::fs::remove_file(&full_path).await;

        return Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to save image.",
        ));
    }

    drop(file);

    let file_size = file_bytes.len() as i64;

    // Save the media metadata to PostgreSQL.
    let result = sqlx::query_as::<_, MediaRecord>(
        r#"
        INSERT INTO media (
            original_filename,
            stored_filename,
            file_path,
            mime_type,
            file_size,
            media_type,
            alt_text,
            caption,
            uploaded_by
        )
        VALUES ($1, $2, $3, $4, $5, 'image', $6, $7, $8)
        RETURNING
            id,
            original_filename,
            mime_type,
            file_size,
            media_type,
            alt_text,
            caption,
            uploaded_by,
            created_at
        "#,
    )
    .bind(&original_filename)
    .bind(&stored_filename)
    .bind(&file_path)
    .bind(mime_type)
    .bind(file_size)
    .bind(&alt_text)
    .bind(&caption)
    .bind(user.id)
    .fetch_one(&state.db)
    .await;

    match result {
        Ok(record) => Ok((StatusCode::CREATED, Json(record))),

        Err(error) => {
            eprintln!("Media database error: {error}");

            // Prevent orphaned files if PostgreSQL insertion fails.
            if let Err(cleanup_error) = tokio::fs::remove_file(&full_path).await {
                eprintln!("Media cleanup error: {cleanup_error}");
            }

            Err(api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to register image in the media library.",
            ))
        }
    }
}

// ======================================================
// STORED MEDIA FILE
// ======================================================

#[derive(sqlx::FromRow)]
struct StoredMediaFile {
    stored_filename: String,
    mime_type: String,
}

// ======================================================
// AUTHENTICATED MEDIA FILE
// ======================================================

// Retrieve an existing image from the media library.
// This route requires authentication.
pub async fn serve_media_file(
    Path(media_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Response, StatusCode> {
    // Retrieve only active image records.
    let media = sqlx::query_as::<_, StoredMediaFile>(
        r#"
        SELECT stored_filename, mime_type
        FROM media
        WHERE id = $1
          AND media_type = 'image'
          AND deleted_at IS NULL
        "#,
    )
    .bind(media_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Media retrieval database error: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    // Allow only the file formats supported by our upload handler.
    let expected_extension = match media.mime_type.as_str() {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        _ => return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE),
    };

    // Ensure the stored filename is a UUID with the expected extension.
    // Never use an arbitrary database path to access files.
    let (filename_id, extension) = media
        .stored_filename
        .rsplit_once('.')
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    Uuid::parse_str(filename_id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if extension != expected_extension {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    // Resolve the file inside the configured media directory.
    let storage_dir =
        std::env::var("MEDIA_STORAGE_DIR").unwrap_or_else(|_| "uploads/media".to_string());

    let file_path = PathBuf::from(storage_dir).join(&media.stored_filename);

    // Read the physical image.
    let bytes = tokio::fs::read(&file_path).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            StatusCode::NOT_FOUND
        } else {
            eprintln!("Media file retrieval error: {error}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;

    // Return the image with safe response headers.
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, media.mime_type)
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CACHE_CONTROL, "private, no-store")
        .header(header::CONTENT_DISPOSITION, "inline")
        .body(Body::from(bytes))
        .map_err(|error| {
            eprintln!("Media response error: {error}");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

// ======================================================
// PUBLIC MEDIA FILE
// ======================================================

/// Serve an image publicly only when it is associated with
/// published or currently active campaign content.
///
/// Supported associations:
/// - Published candidate profile
/// - Published news article
/// - Active headline within its publication dates
/// - Published manifesto priority
/// - Published event
/// - Published campaign poster
///
/// The normal /media/{media_id}/file endpoint remains authenticated.
pub async fn serve_public_media_file(
    Path(media_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Response, StatusCode> {
    let media = sqlx::query_as::<_, StoredMediaFile>(
        r#"
        SELECT
            m.stored_filename,
            m.mime_type
        FROM media m
        WHERE m.id = $1
          AND m.media_type = 'image'
          AND m.deleted_at IS NULL
          AND (
              -- PUBLISHED CANDIDATE PROFILE
              EXISTS (
                  SELECT 1
                  FROM candidate_profile c
                  WHERE c.profile_media_id = m.id
                    AND c.is_published = TRUE
              )

              OR

              -- PUBLISHED NEWS ARTICLE
              EXISTS (
                  SELECT 1
                  FROM news n
                  WHERE n.featured_media_id = m.id
                    AND n.status = 'published'
                    AND n.published_at IS NOT NULL
              )

              OR

              -- ACTIVE HEADLINE WITHIN PUBLICATION DATES
              EXISTS (
                  SELECT 1
                  FROM headlines h
                  WHERE h.media_id = m.id
                    AND h.is_active = TRUE
                    AND (h.starts_at IS NULL OR h.starts_at <= NOW())
                    AND (h.ends_at IS NULL OR h.ends_at > NOW())
              )

              OR

              -- PUBLISHED MANIFESTO PRIORITY
              EXISTS (
                  SELECT 1
                  FROM manifesto_items mi
                  WHERE mi.media_id = m.id
                    AND mi.status = 'published'
              )

              OR

              -- PUBLISHED EVENT
              EXISTS (
                  SELECT 1
                  FROM events e
                  WHERE e.media_id = m.id
                    AND e.is_published = TRUE
                    AND e.published_at IS NOT NULL
              )

              OR

              -- PUBLISHED CAMPAIGN POSTER
              EXISTS (
                  SELECT 1
                  FROM campaign_posters p
                  WHERE p.preview_media_id = m.id
                    AND p.is_published = TRUE
                    AND p.published_at IS NOT NULL
              )
          )
        "#,
    )
    .bind(media_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Public media database error: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    // Allow only the formats accepted by the upload handler.
    let expected_extension = match media.mime_type.as_str() {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        _ => return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE),
    };

    // Validate the stored filename before accessing the filesystem.
    let (filename_id, extension) = media
        .stored_filename
        .rsplit_once('.')
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    Uuid::parse_str(filename_id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if extension != expected_extension {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    // Resolve the file inside the configured media directory.
    let storage_dir = env::var("MEDIA_STORAGE_DIR").unwrap_or_else(|_| "uploads/media".to_string());

    let file_path = PathBuf::from(storage_dir).join(&media.stored_filename);

    let bytes = tokio::fs::read(&file_path).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            StatusCode::NOT_FOUND
        } else {
            eprintln!("Public media file retrieval error: {error}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;

    // Return the image with safe public response headers.
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, media.mime_type)
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CACHE_CONTROL, "public, max-age=60")
        .header(header::CONTENT_DISPOSITION, "inline")
        .body(Body::from(bytes))
        .map_err(|error| {
            eprintln!("Public media response error: {error}");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}
