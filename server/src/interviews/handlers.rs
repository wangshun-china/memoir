use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use uuid::Uuid;

use super::service::{
    continue_session, create_session, finish_session, generate_from_session, get_session,
    list_messages, post_message, CreateInterviewRequest, CreateSessionResponse, GeneratedChapter,
    InterviewMessage, InterviewSession, PostMessageRequest, PostMessageResponse,
};
use crate::auth::AuthUser;
use crate::error::AppResult;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/memoirs/{memoir_id}/interviews", post(create_interview))
        .route("/interviews/{session_id}", get(get_interview))
        .route(
            "/interviews/{session_id}/continue",
            post(continue_interview),
        )
        .route(
            "/interviews/{session_id}/messages",
            get(list_messages_handler).post(post_message_handler),
        )
        .route("/interviews/{session_id}/finish", post(finish_interview))
        .route(
            "/interviews/{session_id}/generate",
            post(generate_interview),
        )
}

async fn continue_interview(
    State(state): State<AppState>,
    user: AuthUser,
    Path(session_id): Path<Uuid>,
) -> AppResult<Json<CreateSessionResponse>> {
    let resp = continue_session(&state.pool, user.user_id, session_id).await?;
    Ok(Json(resp))
}

async fn create_interview(
    State(state): State<AppState>,
    user: AuthUser,
    Path(memoir_id): Path<Uuid>,
    Json(body): Json<CreateInterviewRequest>,
) -> AppResult<Json<CreateSessionResponse>> {
    let resp = create_session(&state.pool, user.user_id, memoir_id, body).await?;
    Ok(Json(resp))
}

async fn get_interview(
    State(state): State<AppState>,
    user: AuthUser,
    Path(session_id): Path<Uuid>,
) -> AppResult<Json<InterviewSession>> {
    let session = get_session(&state.pool, user.user_id, session_id).await?;
    Ok(Json(session))
}

async fn list_messages_handler(
    State(state): State<AppState>,
    user: AuthUser,
    Path(session_id): Path<Uuid>,
) -> AppResult<Json<Vec<InterviewMessage>>> {
    let rows = list_messages(&state.pool, user.user_id, session_id).await?;
    Ok(Json(rows))
}

async fn post_message_handler(
    State(state): State<AppState>,
    user: AuthUser,
    Path(session_id): Path<Uuid>,
    Json(body): Json<PostMessageRequest>,
) -> AppResult<Json<PostMessageResponse>> {
    let resp = post_message(&state, user.user_id, session_id, body).await?;
    Ok(Json(resp))
}

async fn finish_interview(
    State(state): State<AppState>,
    user: AuthUser,
    Path(session_id): Path<Uuid>,
) -> AppResult<Json<InterviewSession>> {
    let session = finish_session(&state.pool, user.user_id, session_id).await?;
    Ok(Json(session))
}

async fn generate_interview(
    State(state): State<AppState>,
    user: AuthUser,
    Path(session_id): Path<Uuid>,
) -> AppResult<Json<GeneratedChapter>> {
    let generated = generate_from_session(&state, user.user_id, session_id, "manual").await?;
    Ok(Json(generated))
}
