use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

/// Default chapter titles from MVP §5.4 — order is the seed order.
pub const DEFAULT_CHAPTER_TITLES: &[&str] = &[
    "童年与家庭",
    "求学经历",
    "青年时代",
    "工作与事业",
    "婚姻与家庭",
    "人生转折",
    "子女与家庭生活",
    "退休与晚年",
    "我想留下的话",
];

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Memoir {
    pub id: Uuid,
    pub owner_user_id: Option<Uuid>,
    pub creator_user_id: Uuid,
    pub title: String,
    pub subject_name: String,
    pub birth_year: Option<i32>,
    pub birth_place: Option<String>,
    pub preferred_name: Option<String>,
    pub creator_relation: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Chapter {
    pub id: Uuid,
    pub memoir_id: Uuid,
    pub title: String,
    pub sort_order: i32,
    pub status: String,
    pub summary: Option<String>,
    /// Generated chapter draft body (from interview transcript).
    pub content: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Chapter row for reader/home UI: includes live interview progress (not only DB status).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ChapterProgress {
    pub id: Uuid,
    pub memoir_id: Uuid,
    pub title: String,
    pub sort_order: i32,
    pub status: String,
    pub summary: Option<String>,
    pub content: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Messages linked by chapter_id or matching topic title.
    pub message_count: i64,
    /// Latest session for this chapter/topic (active preferred).
    pub continue_session_id: Option<Uuid>,
    /// True when any interview transcript exists for this chapter.
    pub has_interview: bool,
    /// True when generated content is non-empty or status is draft/confirmed.
    pub has_draft: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateMemoirRequest {
    pub subject_name: String,
    pub title: Option<String>,
    pub birth_year: Option<i32>,
    pub birth_place: Option<String>,
    pub preferred_name: Option<String>,
    pub creator_relation: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MemoirWithChapters {
    #[serde(flatten)]
    pub memoir: Memoir,
    pub chapters: Vec<Chapter>,
}

pub async fn create_memoir_with_chapters(
    pool: &PgPool,
    creator_user_id: Uuid,
    req: CreateMemoirRequest,
) -> AppResult<MemoirWithChapters> {
    let subject = req.subject_name.trim();
    if subject.is_empty() {
        return Err(AppError::BadRequest("subject_name is required".into()));
    }

    let title = req
        .title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{subject}的回忆录"));

    let mut tx: Transaction<'_, Postgres> = pool.begin().await?;

    let memoir = sqlx::query_as::<_, Memoir>(
        r#"
        INSERT INTO memoirs (
            creator_user_id, title, subject_name, birth_year,
            birth_place, preferred_name, creator_relation
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING *
        "#,
    )
    .bind(creator_user_id)
    .bind(&title)
    .bind(subject)
    .bind(req.birth_year)
    .bind(req.birth_place.as_deref())
    .bind(req.preferred_name.as_deref())
    .bind(req.creator_relation.as_deref())
    .fetch_one(&mut *tx)
    .await?;

    let chapters = seed_default_chapters(&mut tx, memoir.id).await?;
    tx.commit().await?;

    Ok(MemoirWithChapters { memoir, chapters })
}

pub async fn seed_default_chapters(
    tx: &mut Transaction<'_, Postgres>,
    memoir_id: Uuid,
) -> AppResult<Vec<Chapter>> {
    let mut chapters = Vec::with_capacity(DEFAULT_CHAPTER_TITLES.len());
    for (idx, title) in DEFAULT_CHAPTER_TITLES.iter().enumerate() {
        let chapter = sqlx::query_as::<_, Chapter>(
            r#"
            INSERT INTO chapters (memoir_id, title, sort_order, status)
            VALUES ($1, $2, $3, 'empty')
            RETURNING *
            "#,
        )
        .bind(memoir_id)
        .bind(*title)
        .bind((idx + 1) as i32)
        .fetch_one(&mut **tx)
        .await?;
        chapters.push(chapter);
    }
    Ok(chapters)
}

/// Memoir card for home list: includes interview progress so UI can show 开始 vs 继续.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MemoirListItem {
    pub id: Uuid,
    pub owner_user_id: Option<Uuid>,
    pub creator_user_id: Uuid,
    pub title: String,
    pub subject_name: String,
    pub birth_year: Option<i32>,
    pub birth_place: Option<String>,
    pub preferred_name: Option<String>,
    pub creator_relation: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Total interview messages under this memoir (all sessions).
    pub message_count: i64,
    /// True once any interview session exists (even opening-only).
    pub has_interview: bool,
    /// Prefer active session; otherwise latest session with any messages / latest session.
    pub continue_session_id: Option<Uuid>,
    pub continue_topic: Option<String>,
}

pub async fn list_memoirs(pool: &PgPool, user_id: Uuid) -> AppResult<Vec<MemoirListItem>> {
    let rows = sqlx::query_as::<_, MemoirListItem>(
        r#"
        SELECT
          m.id,
          m.owner_user_id,
          m.creator_user_id,
          m.title,
          m.subject_name,
          m.birth_year,
          m.birth_place,
          m.preferred_name,
          m.creator_relation,
          m.status,
          m.created_at,
          m.updated_at,
          COALESCE((
            SELECT COUNT(*)::bigint
            FROM interview_messages im
            JOIN interview_sessions s ON s.id = im.session_id
            WHERE s.memoir_id = m.id
          ), 0) AS message_count,
          (
            EXISTS(SELECT 1 FROM interview_sessions s WHERE s.memoir_id = m.id)
            OR EXISTS(
              SELECT 1 FROM interview_messages im
              JOIN interview_sessions s ON s.id = im.session_id
              WHERE s.memoir_id = m.id
            )
          ) AS has_interview,
          (
            SELECT s.id
            FROM interview_sessions s
            WHERE s.memoir_id = m.id
            ORDER BY
              CASE WHEN s.status = 'active' THEN 0 ELSE 1 END,
              s.started_at DESC
            LIMIT 1
          ) AS continue_session_id,
          (
            SELECT s.topic
            FROM interview_sessions s
            WHERE s.memoir_id = m.id
            ORDER BY
              CASE WHEN s.status = 'active' THEN 0 ELSE 1 END,
              s.started_at DESC
            LIMIT 1
          ) AS continue_topic
        FROM memoirs m
        WHERE m.creator_user_id = $1 OR m.owner_user_id = $1
        ORDER BY m.created_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_memoir(pool: &PgPool, user_id: Uuid, memoir_id: Uuid) -> AppResult<Memoir> {
    let row = sqlx::query_as::<_, Memoir>(
        r#"
        SELECT * FROM memoirs
        WHERE id = $1 AND (creator_user_id = $2 OR owner_user_id = $2)
        "#,
    )
    .bind(memoir_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("memoir not found".into()))?;
    Ok(row)
}

pub async fn list_chapters(
    pool: &PgPool,
    user_id: Uuid,
    memoir_id: Uuid,
) -> AppResult<Vec<ChapterProgress>> {
    // ownership check
    let _ = get_memoir(pool, user_id, memoir_id).await?;
    let rows = sqlx::query_as::<_, ChapterProgress>(
        r#"
        SELECT
          c.id,
          c.memoir_id,
          c.title,
          c.sort_order,
          c.status,
          c.summary,
          c.content,
          c.created_at,
          c.updated_at,
          COALESCE((
            SELECT COUNT(*)::bigint
            FROM interview_messages im
            JOIN interview_sessions s ON s.id = im.session_id
            WHERE s.memoir_id = c.memoir_id
              AND (s.chapter_id = c.id OR s.topic = c.title)
          ), 0) AS message_count,
          (
            SELECT s.id
            FROM interview_sessions s
            WHERE s.memoir_id = c.memoir_id
              AND (s.chapter_id = c.id OR s.topic = c.title)
            ORDER BY
              CASE WHEN s.status = 'active' THEN 0 ELSE 1 END,
              s.started_at DESC
            LIMIT 1
          ) AS continue_session_id,
          (
            EXISTS(
              SELECT 1 FROM interview_sessions s
              WHERE s.memoir_id = c.memoir_id
                AND (s.chapter_id = c.id OR s.topic = c.title)
            )
            OR EXISTS(
              SELECT 1
              FROM interview_messages im
              JOIN interview_sessions s ON s.id = im.session_id
              WHERE s.memoir_id = c.memoir_id
                AND (s.chapter_id = c.id OR s.topic = c.title)
            )
          ) AS has_interview,
          (
            (c.content IS NOT NULL AND length(trim(c.content)) > 0)
            OR c.status IN ('draft', 'confirmed')
          ) AS has_draft
        FROM chapters c
        WHERE c.memoir_id = $1
        ORDER BY c.sort_order ASC
        "#,
    )
    .bind(memoir_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Soft ownership delete: cascades sessions/messages/chapters via FK.
pub async fn delete_memoir(pool: &PgPool, user_id: Uuid, memoir_id: Uuid) -> AppResult<()> {
    let res = sqlx::query(
        r#"
        DELETE FROM memoirs
        WHERE id = $1 AND (creator_user_id = $2 OR owner_user_id = $2)
        "#,
    )
    .bind(memoir_id)
    .bind(user_id)
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("memoir not found".into()));
    }
    Ok(())
}
