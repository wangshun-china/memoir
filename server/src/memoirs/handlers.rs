use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use uuid::Uuid;

use super::service::{
    create_memoir_with_chapters, delete_memoir, get_memoir, list_chapters, list_memoirs,
    ChapterProgress, CreateMemoirRequest, Memoir, MemoirListItem, MemoirWithChapters,
};
use crate::auth::AuthUser;
use crate::error::AppResult;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/memoirs", post(create_memoir).get(list_memoirs_handler))
        .route(
            "/memoirs/{memoir_id}",
            get(get_memoir_handler).delete(delete_memoir_handler),
        )
        .route("/memoirs/{memoir_id}/chapters", get(list_chapters_handler))
}

async fn create_memoir(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<CreateMemoirRequest>,
) -> AppResult<Json<MemoirWithChapters>> {
    let result = create_memoir_with_chapters(&state.pool, user.user_id, body).await?;
    Ok(Json(result))
}

async fn list_memoirs_handler(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<MemoirListItem>>> {
    let rows = list_memoirs(&state.pool, user.user_id).await?;
    Ok(Json(rows))
}

async fn get_memoir_handler(
    State(state): State<AppState>,
    user: AuthUser,
    Path(memoir_id): Path<Uuid>,
) -> AppResult<Json<Memoir>> {
    let row = get_memoir(&state.pool, user.user_id, memoir_id).await?;
    Ok(Json(row))
}

async fn delete_memoir_handler(
    State(state): State<AppState>,
    user: AuthUser,
    Path(memoir_id): Path<Uuid>,
) -> AppResult<StatusCode> {
    delete_memoir(&state.pool, user.user_id, memoir_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_chapters_handler(
    State(state): State<AppState>,
    user: AuthUser,
    Path(memoir_id): Path<Uuid>,
) -> AppResult<Json<Vec<ChapterProgress>>> {
    let rows = list_chapters(&state.pool, user.user_id, memoir_id).await?;
    Ok(Json(rows))
}
