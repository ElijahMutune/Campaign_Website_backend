use axum::{
    Router, middleware,
    routing::{get, post},
};

use crate::{
    handlers::auth_handler::{change_password, login, me},
    middleware::auth_middleware::require_auth,
    state::AppState,
};

pub fn router(state: AppState) -> Router<AppState> {
    let protected_routes = Router::new()
        .route("/me", get(me))
        .route("/change-password", post(change_password))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .route("/login", post(login))
        .merge(protected_routes)
}
