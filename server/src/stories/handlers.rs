use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use uuid::Uuid;

use super::service::{
    confirm_story, extract_story_from_session, list_stories, organize_memoir, OrganizeResult,
    StoryCard,
};
use crate::auth::AuthUser;
use crate::error::AppResult;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/interviews/{session_id}/stories",
            post(extract_story_handler),
        )
        .route("/memoirs/{memoir_id}/stories", get(list_stories_handler))
        .route("/stories/{story_id}/confirm", post(confirm_story_handler))
        .route("/memoirs/{memoir_id}/organize", post(organize_handler))
}

async fn extract_story_handler(
    State(state): State<AppState>,
    user: AuthUser,
    Path(session_id): Path<Uuid>,
) -> AppResult<Json<StoryCard>> {
    Ok(Json(
        extract_story_from_session(&state, user.user_id, session_id).await?,
    ))
}

async fn list_stories_handler(
    State(state): State<AppState>,
    user: AuthUser,
    Path(memoir_id): Path<Uuid>,
) -> AppResult<Json<Vec<StoryCard>>> {
    Ok(Json(
        list_stories(&state.pool, user.user_id, memoir_id).await?,
    ))
}

async fn confirm_story_handler(
    State(state): State<AppState>,
    user: AuthUser,
    Path(story_id): Path<Uuid>,
) -> AppResult<Json<StoryCard>> {
    Ok(Json(
        confirm_story(&state.pool, user.user_id, story_id).await?,
    ))
}

async fn organize_handler(
    State(state): State<AppState>,
    user: AuthUser,
    Path(memoir_id): Path<Uuid>,
) -> AppResult<Json<OrganizeResult>> {
    Ok(Json(
        organize_memoir(&state, user.user_id, memoir_id).await?,
    ))
}
