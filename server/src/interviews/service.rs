use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::llm::{chapter as chapter_llm, interviewer};
use crate::memoirs::service::{get_memoir, Chapter};
use crate::state::AppState;

/// User turns (role=user) that trigger automatic chapter generation.
pub const AUTO_GENERATE_USER_TURNS: i64 = 20;

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
    pub auto_generated_at: Option<DateTime<Utc>>,
    /// idle | generating | ready | failed
    pub generation_status: String,
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
    /// When true, always create a new session (explicit「开始采访」). Default false = resume.
    pub force_new: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct PostMessageRequest {
    pub content: String,
    /// Optional client action: normal | dont_know | change_question | prefer_not | end
    pub action: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GeneratedChapter {
    pub chapter: Chapter,
    pub session_summary: Option<String>,
    pub trigger: String,
    pub user_turn_count: i64,
}

#[derive(Debug, Serialize)]
pub struct PostMessageResponse {
    pub user_message: InterviewMessage,
    pub assistant_message: Option<InterviewMessage>,
    pub session_status: String,
    pub user_turn_count: i64,
    pub auto_generate_at: i64,
    /// Populated only when generation finished inline (rare); auto-gen is usually async.
    pub generated: Option<GeneratedChapter>,
    /// True when auto chapter generation was scheduled in the background.
    pub generation_started: bool,
    pub generation_status: String,
}

#[derive(Debug, Serialize)]
pub struct CreateSessionResponse {
    pub session: InterviewSession,
    pub opening_message: Option<InterviewMessage>,
    pub resumed: bool,
    pub user_turn_count: i64,
    pub auto_generate_at: i64,
}

pub async fn create_session(
    pool: &PgPool,
    user_id: Uuid,
    memoir_id: Uuid,
    req: CreateInterviewRequest,
) -> AppResult<CreateSessionResponse> {
    let memoir = get_memoir(pool, user_id, memoir_id).await?;

    let topic = req
        .topic
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("童年与家庭")
        .to_string();

    let force_new = req.force_new.unwrap_or(false);

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

    // Resume: prefer active same-topic, else latest session on this memoir (reopen if finished).
    // force_new is only for explicit「开始采访」when there is no history.
    if !force_new {
        if let Some(existing) = find_resumable_session(pool, memoir_id, &topic).await? {
            let session = reopen_if_needed(pool, existing).await?;
            let turns = count_user_turns(pool, session.id).await?;
            return Ok(CreateSessionResponse {
                session,
                opening_message: None,
                resumed: true,
                user_turn_count: turns,
                auto_generate_at: AUTO_GENERATE_USER_TURNS,
            });
        }
    }

    // Bind chapter by matching topic title when not provided.
    let chapter_id = if req.chapter_id.is_some() {
        req.chapter_id
    } else {
        sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT id FROM chapters
            WHERE memoir_id = $1 AND title = $2
            ORDER BY sort_order ASC
            LIMIT 1
            "#,
        )
        .bind(memoir_id)
        .bind(&topic)
        .fetch_optional(pool)
        .await?
    };

    let session = sqlx::query_as::<_, InterviewSession>(
        r#"
        INSERT INTO interview_sessions (memoir_id, chapter_id, topic, status)
        VALUES ($1, $2, $3, 'active')
        RETURNING *
        "#,
    )
    .bind(memoir_id)
    .bind(chapter_id)
    .bind(&topic)
    .fetch_one(pool)
    .await?;

    let opening = format!(
        "您好{}。我们今天聊聊「{}」。{}",
        preferred_address(&memoir.preferred_name, &memoir.subject_name),
        topic,
        opening_hook_for_topic(&topic)
    );

    let assistant =
        insert_message(pool, session.id, "assistant", &opening, Some("opening")).await?;

    // Mark chapter as collecting when interview starts.
    if let Some(cid) = chapter_id {
        let _ = sqlx::query(
            r#"
            UPDATE chapters
            SET status = CASE WHEN status = 'empty' THEN 'collecting' ELSE status END,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(cid)
        .execute(pool)
        .await;
    }

    Ok(CreateSessionResponse {
        session,
        opening_message: Some(assistant),
        resumed: false,
        user_turn_count: 0,
        auto_generate_at: AUTO_GENERATE_USER_TURNS,
    })
}

/// Prefer active session for topic, then any active on memoir, then latest session overall.
async fn find_resumable_session(
    pool: &PgPool,
    memoir_id: Uuid,
    topic: &str,
) -> AppResult<Option<InterviewSession>> {
    if let Some(row) = sqlx::query_as::<_, InterviewSession>(
        r#"
        SELECT *
        FROM interview_sessions
        WHERE memoir_id = $1 AND topic = $2 AND status = 'active'
        ORDER BY started_at DESC
        LIMIT 1
        "#,
    )
    .bind(memoir_id)
    .bind(topic)
    .fetch_optional(pool)
    .await?
    {
        return Ok(Some(row));
    }

    if let Some(row) = sqlx::query_as::<_, InterviewSession>(
        r#"
        SELECT *
        FROM interview_sessions
        WHERE memoir_id = $1 AND status = 'active'
        ORDER BY started_at DESC
        LIMIT 1
        "#,
    )
    .bind(memoir_id)
    .fetch_optional(pool)
    .await?
    {
        return Ok(Some(row));
    }

    let row = sqlx::query_as::<_, InterviewSession>(
        r#"
        SELECT *
        FROM interview_sessions
        WHERE memoir_id = $1
        ORDER BY started_at DESC
        LIMIT 1
        "#,
    )
    .bind(memoir_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Re-open a finished session so the user can continue the same transcript.
pub async fn reopen_if_needed(
    pool: &PgPool,
    session: InterviewSession,
) -> AppResult<InterviewSession> {
    if session.status == "active" {
        return Ok(session);
    }
    let row = sqlx::query_as::<_, InterviewSession>(
        r#"
        UPDATE interview_sessions
        SET status = 'active', finished_at = NULL
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(session.id)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// Continue a known session by id (does not create a new session).
pub async fn continue_session(
    pool: &PgPool,
    user_id: Uuid,
    session_id: Uuid,
) -> AppResult<CreateSessionResponse> {
    let session = get_session(pool, user_id, session_id).await?;
    let session = reopen_if_needed(pool, session).await?;
    let turns = count_user_turns(pool, session.id).await?;
    Ok(CreateSessionResponse {
        session,
        opening_message: None,
        resumed: true,
        user_turn_count: turns,
        auto_generate_at: AUTO_GENERATE_USER_TURNS,
    })
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

/// Topic-specific first question — avoid always asking about "the place you lived".
pub fn opening_hook_for_topic(topic: &str) -> &'static str {
    match topic.trim() {
        "童年与家庭" => {
            "先从一个具体的小地方开始：您还记得小时候住的地方，门口或院子是什么样的吗？"
        }
        "求学经历" => "先从一个具体的小事开始：您还记得第一次去学校那天，路上或教室里是什么情形吗？",
        "青年时代" => "先从一个具体的画面开始：青年时有没有一件事，让您觉得「自己长大了」？",
        "工作与事业" => {
            "先从一个具体的岗位开始：您还记得第一份工作，或者第一天上班时在做什么吗？"
        }
        "婚姻与家庭" => {
            "先从一个具体的时刻开始：您和伴侣是怎么认识的，还是第一次成家时家里是什么样子？"
        }
        "人生转折" => {
            "先从一个具体的岔路口开始：人生里有没有哪一次选择或变故，后来想起来影响特别大？"
        }
        "子女与家庭生活" => {
            "先从一个具体的画面开始：孩子小时候，您印象最深的一件日常小事是什么？"
        }
        "退休与晚年" => {
            "先从一个具体的变化开始：退休或年岁渐长之后，您每天的日子和从前有什么不一样？"
        }
        "我想留下的话" => {
            "这一章想听听您最想留给家人的话。先不必写很长：如果现在只能说一句，您最想对亲人说什么？"
        }
        _ => "先从一个具体、好回答的小事开始：关于这个话题，您最先想到的一件事是什么？",
    }
}

#[cfg(test)]
mod opening_tests {
    use super::opening_hook_for_topic;

    #[test]
    fn leaving_words_topic_not_about_childhood_home() {
        let hook = opening_hook_for_topic("我想留下的话");
        assert!(hook.contains("留给") || hook.contains("亲人") || hook.contains("一句"));
        assert!(!hook.contains("住的地方"));
        assert!(!hook.contains("门口"));
    }

    #[test]
    fn childhood_still_can_ask_about_home() {
        let hook = opening_hook_for_topic("童年与家庭");
        assert!(hook.contains("小时候") || hook.contains("门口") || hook.contains("院子"));
    }
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

/// Recent messages only (for interviewer context). Still ownership-checked.
pub async fn list_messages_recent(
    pool: &PgPool,
    user_id: Uuid,
    session_id: Uuid,
    limit: i64,
) -> AppResult<Vec<InterviewMessage>> {
    let _ = get_session(pool, user_id, session_id).await?;
    let rows = sqlx::query_as::<_, InterviewMessage>(
        r#"
        SELECT * FROM (
          SELECT * FROM interview_messages
          WHERE session_id = $1
          ORDER BY created_at DESC, id DESC
          LIMIT $2
        ) t
        ORDER BY created_at ASC, id ASC
        "#,
    )
    .bind(session_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn count_user_turns(pool: &PgPool, session_id: Uuid) -> AppResult<i64> {
    let n = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*) FROM interview_messages
        WHERE session_id = $1 AND role = 'user'
        "#,
    )
    .bind(session_id)
    .fetch_one(pool)
    .await?;
    Ok(n)
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
        let content = if req.content.trim().is_empty() {
            "结束本次采访".to_string()
        } else {
            req.content.trim().to_string()
        };
        let user_message =
            insert_message(&state.pool, session_id, "user", &content, Some("end")).await?;
        let turns = count_user_turns(&state.pool, session_id).await?;
        let session = finish_session(&state.pool, user_id, session_id).await?;
        return Ok(PostMessageResponse {
            user_message,
            assistant_message: None,
            session_status: session.status,
            user_turn_count: turns,
            auto_generate_at: AUTO_GENERATE_USER_TURNS,
            generated: None,
            generation_started: false,
            generation_status: session.generation_status,
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

    // 1) Persist user turn first — every message is stored in interview_messages.
    let user_message = insert_message(
        &state.pool,
        session_id,
        "user",
        &stored_content,
        Some(action),
    )
    .await?;

    // Recent window only — full history is still in DB for generate/list UI.
    let history = list_messages_recent(&state.pool, user_id, session_id, 16).await?;
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

    // Mark chapter collecting once real dialogue exists.
    if let Some(cid) = session.chapter_id {
        let _ = sqlx::query(
            r#"
            UPDATE chapters
            SET status = CASE WHEN status IN ('empty', 'collecting') THEN 'collecting' ELSE status END,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(cid)
        .execute(&state.pool)
        .await;
    }

    let turns = count_user_turns(&state.pool, session_id).await?;

    // 4) Auto-generate in background when threshold reached (do not block chat reply).
    let mut generation_started = false;
    let mut generation_status = session.generation_status.clone();
    if turns >= AUTO_GENERATE_USER_TURNS {
        if claim_generation_slot(&state.pool, session_id, true).await? {
            generation_started = true;
            generation_status = "generating".into();
            let state_bg = state.clone();
            tokio::spawn(async move {
                if let Err(e) =
                    generate_from_session(&state_bg, user_id, session_id, "auto").await
                {
                    tracing::warn!(error = %e, %session_id, "async auto chapter generation failed");
                    let _ = mark_generation_status(&state_bg.pool, session_id, "failed").await;
                }
            });
        } else {
            let sess = get_session(&state.pool, user_id, session_id).await?;
            generation_status = sess.generation_status;
        }
    }

    Ok(PostMessageResponse {
        user_message,
        assistant_message: Some(assistant_message),
        session_status: "active".into(),
        user_turn_count: turns,
        auto_generate_at: AUTO_GENERATE_USER_TURNS,
        generated: None,
        generation_started,
        generation_status,
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

/// Claim generation slot. `auto_only_once`: also require `auto_generated_at IS NULL`.
async fn claim_generation_slot(
    pool: &PgPool,
    session_id: Uuid,
    auto_only_once: bool,
) -> AppResult<bool> {
    let row = if auto_only_once {
        sqlx::query_scalar::<_, Uuid>(
            r#"
            UPDATE interview_sessions
            SET generation_status = 'generating'
            WHERE id = $1
              AND generation_status <> 'generating'
              AND auto_generated_at IS NULL
            RETURNING id
            "#,
        )
        .bind(session_id)
        .fetch_optional(pool)
        .await?
    } else {
        sqlx::query_scalar::<_, Uuid>(
            r#"
            UPDATE interview_sessions
            SET generation_status = 'generating'
            WHERE id = $1
              AND generation_status <> 'generating'
            RETURNING id
            "#,
        )
        .bind(session_id)
        .fetch_optional(pool)
        .await?
    };
    Ok(row.is_some())
}

async fn mark_generation_status(pool: &PgPool, session_id: Uuid, status: &str) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE interview_sessions
        SET generation_status = $2
        WHERE id = $1
        "#,
    )
    .bind(session_id)
    .bind(status)
    .execute(pool)
    .await?;
    Ok(())
}

/// Manually (or auto) generate chapter draft from this session's messages and persist to DB.
pub async fn generate_from_session(
    state: &AppState,
    user_id: Uuid,
    session_id: Uuid,
    trigger: &str,
) -> AppResult<GeneratedChapter> {
    let session = get_session(&state.pool, user_id, session_id).await?;

    // Manual HTTP must claim. Async auto path already claimed before spawn.
    if trigger != "auto" {
        let claimed = claim_generation_slot(&state.pool, session_id, false).await?;
        if !claimed {
            return Err(AppError::Conflict("章节正在生成中，请稍候".into()));
        }
    }

    let memoir = get_memoir(&state.pool, user_id, session.memoir_id).await?;
    let history = list_messages(&state.pool, user_id, session_id).await?;
    let turns = count_user_turns(&state.pool, session_id).await?;

    if history.iter().filter(|m| m.role == "user").count() == 0 {
        let _ = mark_generation_status(&state.pool, session_id, "idle").await;
        return Err(AppError::BadRequest(
            "还没有对话内容，请先回答几个问题再生成".into(),
        ));
    }

    let client = {
        let runtime = state.llm_runtime.read().await;
        runtime.client.clone()
    };

    let draft = match chapter_llm::generate_chapter_draft(
        client.as_ref(),
        &session.topic,
        &memoir.subject_name,
        &history,
    )
    .await
    {
        Ok(c) => {
            let _ = crate::settings::record_usage(
                &state.pool,
                "chapter_generate",
                &c.model,
                c.prompt_tokens,
                c.completion_tokens,
                c.total_tokens,
                c.latency_ms,
                true,
                None,
            )
            .await;
            c.content
        }
        Err(e) => {
            let err = e.to_string();
            tracing::error!(error = %err, "LLM chapter generate failed; using transcript fallback");
            let _ = crate::settings::record_usage(
                &state.pool,
                "chapter_generate",
                client.model_name(),
                0,
                0,
                0,
                0,
                false,
                Some(&err),
            )
            .await;
            chapter_llm::fallback_chapter_draft(&session.topic, &memoir.subject_name, &history)
        }
    };

    let summary: String = draft.chars().take(200).collect();
    let chapter_id = match resolve_chapter_id(&state.pool, &session).await {
        Ok(id) => id,
        Err(e) => {
            let _ = mark_generation_status(&state.pool, session_id, "failed").await;
            return Err(e);
        }
    };

    let chapter = sqlx::query_as::<_, Chapter>(
        r#"
        UPDATE chapters
        SET content = $2,
            summary = $3,
            status = 'draft',
            updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(chapter_id)
    .bind(&draft)
    .bind(&summary)
    .fetch_one(&state.pool)
    .await?;

    let session_summary = Some(summary.clone());
    sqlx::query(
        r#"
        UPDATE interview_sessions
        SET summary = $2,
            chapter_id = COALESCE(chapter_id, $3),
            generation_status = 'ready',
            auto_generated_at = CASE
              WHEN $4 THEN COALESCE(auto_generated_at, NOW())
              ELSE auto_generated_at
            END
        WHERE id = $1
        "#,
    )
    .bind(session_id)
    .bind(&summary)
    .bind(chapter_id)
    .bind(trigger == "auto")
    .execute(&state.pool)
    .await?;

    Ok(GeneratedChapter {
        chapter,
        session_summary,
        trigger: trigger.to_string(),
        user_turn_count: turns,
    })
}

async fn resolve_chapter_id(pool: &PgPool, session: &InterviewSession) -> AppResult<Uuid> {
    if let Some(id) = session.chapter_id {
        return Ok(id);
    }
    let id = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT id FROM chapters
        WHERE memoir_id = $1 AND title = $2
        ORDER BY sort_order ASC
        LIMIT 1
        "#,
    )
    .bind(session.memoir_id)
    .bind(&session.topic)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("matching chapter not found for topic".into()))?;
    Ok(id)
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
