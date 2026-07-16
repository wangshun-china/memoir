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
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
        .unwrap_or_else(|| format!("{}的回忆录", subject));

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

pub async fn list_memoirs(pool: &PgPool, user_id: Uuid) -> AppResult<Vec<Memoir>> {
    let rows = sqlx::query_as::<_, Memoir>(
        r#"
        SELECT * FROM memoirs
        WHERE creator_user_id = $1 OR owner_user_id = $1
        ORDER BY created_at DESC
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
) -> AppResult<Vec<Chapter>> {
    // ownership check
    let _ = get_memoir(pool, user_id, memoir_id).await?;
    let rows = sqlx::query_as::<_, Chapter>(
        r#"
        SELECT * FROM chapters
        WHERE memoir_id = $1
        ORDER BY sort_order ASC
        "#,
    )
    .bind(memoir_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
