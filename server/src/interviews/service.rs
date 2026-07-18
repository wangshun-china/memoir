use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::llm::interviewer;
use crate::memoirs::service::get_memoir;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct InterviewSession {
    pub id: Uuid,
    pub memoir_id: Uuid,
    pub chapter_id: Option<Uuid>,
    pub topic: String,
    pub status: String,
    pub summary: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct InterviewMessage {
    pub id: Uuid,
    pub session_id: Uuid,
    pub role: String,
    pub content: String,
    pub question_type: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateInterviewRequest {
    pub topic: Option<String>,
    pub chapter_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct PostMessageRequest {
    pub content: String,
    /// Optional client action: normal | dont_know | change_question | prefer_not | end
    pub action: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PostMessageResponse {
    pub user_message: InterviewMessage,
    pub assistant_message: Option<InterviewMessage>,
    pub session_status: String,
}

pub async fn create_session(
    pool: &PgPool,
    user_id: Uuid,
    memoir_id: Uuid,
    req: CreateInterviewRequest,
) -> AppResult<(InterviewSession, InterviewMessage)> {
    let memoir = get_memoir(pool, user_id, memoir_id).await?;

    let topic = req
        .topic
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("童年与家庭")
        .to_string();

    if let Some(chapter_id) = req.chapter_id {
        let owns = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS(
              SELECT 1 FROM chapters c
              JOIN memoirs m ON m.id = c.memoir_id
              WHERE c.id = $1 AND m.id = $2
                AND (m.creator_user_id = $3 OR m.owner_user_id = $3)
            )
            "#,
        )
        .bind(chapter_id)
        .bind(memoir_id)
        .bind(user_id)
        .fetch_one(pool)
        .await?;
        if !owns {
            return Err(AppError::NotFound("chapter not found".into()));
        }
    }

    let session = sqlx::query_as::<_, InterviewSession>(
        r#"
        INSERT INTO interview_sessions (memoir_id, chapter_id, topic, status)
        VALUES ($1, $2, $3, 'active')
        RETURNING *
        "#,
    )
    .bind(memoir_id)
    .bind(req.chapter_id)
    .bind(&topic)
    .fetch_one(pool)
    .await?;

    let opening = format!(
        "您好{}。我们今天聊聊「{}」。先从一个具体的小地方开始：您还记得那时候住的地方是什么样的吗？",
        preferred_address(&memoir.preferred_name, &memoir.subject_name),
        topic
    );

    let assistant =
        insert_message(pool, session.id, "assistant", &opening, Some("opening")).await?;
    Ok((session, assistant))
}

fn preferred_address(preferred: &Option<String>, subject: &str) -> String {
    preferred
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("，{s}"))
        .unwrap_or_else(|| {
            if subject.is_empty() {
                String::new()
            } else {
                format!("，{subject}")
            }
        })
}

pub async fn get_session(
    pool: &PgPool,
    user_id: Uuid,
    session_id: Uuid,
) -> AppResult<InterviewSession> {
    let session = sqlx::query_as::<_, InterviewSession>(
        r#"
        SELECT s.* FROM interview_sessions s
        JOIN memoirs m ON m.id = s.memoir_id
        WHERE s.id = $1 AND (m.creator_user_id = $2 OR m.owner_user_id = $2)
        "#,
    )
    .bind(session_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("interview session not found".into()))?;
    Ok(session)
}

pub async fn list_messages(
    pool: &PgPool,
    user_id: Uuid,
    session_id: Uuid,
) -> AppResult<Vec<InterviewMessage>> {
    let _ = get_session(pool, user_id, session_id).await?;
    let rows = sqlx::query_as::<_, InterviewMessage>(
        r#"
        SELECT * FROM interview_messages
        WHERE session_id = $1
        ORDER BY created_at ASC, id ASC
        "#,
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn post_message(
    state: &AppState,
    user_id: Uuid,
    session_id: Uuid,
    req: PostMessageRequest,
) -> AppResult<PostMessageResponse> {
    let session = get_session(&state.pool, user_id, session_id).await?;
    if session.status != "active" {
        return Err(AppError::Conflict("session is not active".into()));
    }

    let action = req.action.as_deref().unwrap_or("normal");
    if action == "end" {
        // Persist optional farewell note as user message if content provided.
        let content = if req.content.trim().is_empty() {
            "结束本次采访".to_string()
        } else {
            req.content.trim().to_string()
        };
        let user_message =
            insert_message(&state.pool, session_id, "user", &content, Some("end")).await?;
        let session = finish_session(&state.pool, user_id, session_id).await?;
        return Ok(PostMessageResponse {
            user_message,
            assistant_message: None,
            session_status: session.status,
        });
    }

    let (stored_content, llm_user_text) = match action {
        "dont_know" => (
            "不知道怎么回答".to_string(),
            "__skip_dont_know__".to_string(),
        ),
        "change_question" => (
            "换一个问题".to_string(),
            "__skip_change_question__".to_string(),
        ),
        "prefer_not" => (
            "这个问题不想说".to_string(),
            "__skip_prefer_not__".to_string(),
        ),
        _ => {
            let c = req.content.trim();
            if c.is_empty() {
                return Err(AppError::BadRequest("content is required".into()));
            }
            if c.chars().count() > 4000 {
                return Err(AppError::BadRequest("content too long".into()));
            }
            (c.to_string(), c.to_string())
        }
    };

    // 1) Persist user turn first, release connection before LLM.
    let user_message = insert_message(
        &state.pool,
        session_id,
        "user",
        &stored_content,
        Some(action),
    )
    .await?;

    let history = list_messages(&state.pool, user_id, session_id).await?;
    let memoir = get_memoir(&state.pool, user_id, session.memoir_id).await?;

    // 2) Call interviewer skill without holding a DB connection.
    let client = {
        let runtime = state.llm_runtime.read().await;
        runtime.client.clone()
    };
    let completion = match interviewer::next_question(
        client.as_ref(),
        &session.topic,
        &memoir.subject_name,
        &history,
        &llm_user_text,
    )
    .await
    {
        Ok(c) => {
            let _ = crate::settings::record_usage(
                &state.pool,
                "interview",
                &c.model,
                c.prompt_tokens,
                c.completion_tokens,
                c.total_tokens,
                c.latency_ms,
                true,
                None,
            )
            .await;
            c
        }
        Err(e) => {
            // Keep full error in DB usage logs for admin diagnosis; user still gets a soft follow-up.
            let err = e.to_string();
            tracing::error!(error = %err, "LLM failed during interview; using soft recovery reply");
            let _ = crate::settings::record_usage(
                &state.pool,
                "interview",
                client.model_name(),
                0,
                0,
                0,
                0,
                false,
                Some(&err),
            )
            .await;
            crate::llm::client::LlmCompletion {
                content: "刚才我这边信号不好，没听清。您能换个说法，再讲一遍刚才那件事吗？".into(),
                model: client.model_name().to_string(),
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
                latency_ms: 0,
                used_fallback: true,
            }
        }
    };

    // 3) Persist assistant turn.
    let assistant_message = insert_message(
        &state.pool,
        session_id,
        "assistant",
        &completion.content,
        Some("follow_up"),
    )
    .await?;

    Ok(PostMessageResponse {
        user_message,
        assistant_message: Some(assistant_message),
        session_status: "active".into(),
    })
}

pub async fn finish_session(
    pool: &PgPool,
    user_id: Uuid,
    session_id: Uuid,
) -> AppResult<InterviewSession> {
    let _ = get_session(pool, user_id, session_id).await?;
    let session = sqlx::query_as::<_, InterviewSession>(
        r#"
        UPDATE interview_sessions
        SET status = 'finished', finished_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(session_id)
    .fetch_one(pool)
    .await?;
    Ok(session)
}

async fn insert_message(
    pool: &PgPool,
    session_id: Uuid,
    role: &str,
    content: &str,
    question_type: Option<&str>,
) -> AppResult<InterviewMessage> {
    let row = sqlx::query_as::<_, InterviewMessage>(
        r#"
        INSERT INTO interview_messages (session_id, role, content, question_type)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(session_id)
    .bind(role)
    .bind(content)
    .bind(question_type)
    .fetch_one(pool)
    .await?;
    Ok(row)
}
