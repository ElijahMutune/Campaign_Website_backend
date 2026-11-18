use axum::{
    Router,
    extract::DefaultBodyLimit,
    middleware,
    routing::{get, post},
};

use crate::{
    handlers::{
        media_album_handler::{
            add_media_to_album, create_album, get_album, list_albums, remove_media_from_album,
            update_album,
        },
        media_handler::{serve_media_file, upload_media},
        media_list_handler::list_media,
    },
    middleware::auth_middleware::require_password_changed,
    state::AppState,
};

const MAX_REQUEST_BYTES: usize = (8 * 1024 * 1024) + (64 * 1024);

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/upload", post(upload_media))
        .route("/list", get(list_media))
        .route("/{media_id}/file", get(serve_media_file))
        .route("/albums", get(list_albums).post(create_album))
        .route("/albums/{album_id}", get(get_album).put(update_album))
        .route(
            "/albums/{album_id}/media/{media_id}",
            post(add_media_to_album).delete(remove_media_from_album),
        )
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
        .route_layer(middleware::from_fn_with_state(
            state,
            require_password_changed,
        ))
}
