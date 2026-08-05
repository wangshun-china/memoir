use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::interviews::service::{get_session, list_messages, InterviewMessage};
use crate::llm::client::{ChatMessage, CompleteOptions};
use crate::memoirs::service::{get_memoir, Chapter, DEFAULT_CHAPTER_TITLES};
use crate::state::AppState;

const STORY_EXTRACT_PROMPT: &str = r#"你是严谨的口述史资料整理员。请从一次短采访中提取一张故事卡片。
只能使用对话明确出现的事实，不得补充年份、地点、人物、心理或因果。
如果时间不确定，year_start/year_end 必须为 null，time_precision 使用 unknown 或 approximate。
recommended_chapter 必须从给出的章节名称中选择。
只输出一个 JSON 对象，不要 Markdown：
{"title":"","summary":"","narrative":"","life_stage":"","time_text":"","year_start":null,"year_end":null,"time_precision":"unknown","location_text":"","people":[],"themes":[],"emotions":[],"missing_details":[],"recommended_chapter":""}"#;

const CHAPTER_FROM_STORIES_PROMPT: &str = r#"你是一位严谨的回忆录编辑。把多张已确认故事卡片整理成一个连贯章节。
只可使用卡片中的事实，不得虚构未提供的细节。保留讲述人的口语与情感；删掉重复内容，可按时间或场景分段。
只输出章节正文，不输出说明、JSON 或“以下是草稿”等元话语。"#;

#[derive(Debug, Clone, Serialize)]
pub struct StoryCard {
    pub id: Uuid,
    pub memoir_id: Uuid,
    pub session_id: Option<Uuid>,
    pub title: String,
    pub summary: String,
    pub narrative: String,
    pub life_stage: Option<String>,
    pub time_text: Option<String>,
    pub year_start: Option<i32>,
    pub year_end: Option<i32>,
    pub time_precision: String,
    pub location_text: Option<String>,
    pub people: Vec<String>,
    pub themes: Vec<String>,
    pub emotions: Vec<String>,
    pub missing_details: Vec<String>,
    pub primary_chapter_id: Option<Uuid>,
    pub primary_chapter_title: Option<String>,
    pub status: String,
    pub source_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct StoryRow {
    id: Uuid,
    memoir_id: Uuid,
    session_id: Option<Uuid>,
    title: String,
    summary: String,
    narrative: String,
    life_stage: Option<String>,
    time_text: Option<String>,
    year_start: Option<i32>,
    year_end: Option<i32>,
    time_precision: String,
    location_text: Option<String>,
    people_json: String,
    themes_json: String,
    emotions_json: String,
    missing_details_json: String,
    primary_chapter_id: Option<Uuid>,
    primary_chapter_title: Option<String>,
    status: String,
    source_count: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct ExtractedStory {
    title: Option<String>,
    summary: Option<String>,
    narrative: Option<String>,
    life_stage: Option<String>,
    time_text: Option<String>,
    year_start: Option<i32>,
    year_end: Option<i32>,
    time_precision: Option<String>,
    location_text: Option<String>,
    #[serde(default)]
    people: Vec<String>,
    #[serde(default)]
    themes: Vec<String>,
    #[serde(default)]
    emotions: Vec<String>,
    #[serde(default)]
    missing_details: Vec<String>,
    recommended_chapter: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelineEvent {
    pub story_id: Uuid,
    pub title: String,
    pub time_text: Option<String>,
    pub year_start: Option<i32>,
    pub year_end: Option<i32>,
    pub time_precision: String,
    pub summary: String,
}

#[derive(Debug, Serialize)]
pub struct OrganizeResult {
    pub timeline: Vec<TimelineEvent>,
    pub chapters: Vec<Chapter>,
    pub story_count: usize,
}

impl From<StoryRow> for StoryCard {
    fn from(row: StoryRow) -> Self {
        Self {
            id: row.id,
            memoir_id: row.memoir_id,
            session_id: row.session_id,
            title: row.title,
            summary: row.summary,
            narrative: row.narrative,
            life_stage: row.life_stage,
            time_text: row.time_text,
            year_start: row.year_start,
            year_end: row.year_end,
            time_precision: row.time_precision,
            location_text: row.location_text,
            people: decode_list(&row.people_json),
            themes: decode_list(&row.themes_json),
            emotions: decode_list(&row.emotions_json),
            missing_details: decode_list(&row.missing_details_json),
            primary_chapter_id: row.primary_chapter_id,
            primary_chapter_title: row.primary_chapter_title,
            status: row.status,
            source_count: row.source_count,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

fn decode_list(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn clean_list(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .take(12)
        .collect()
}

fn parse_json_object(raw: &str) -> Option<ExtractedStory> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    serde_json::from_str(&raw[start..=end]).ok()
}

fn substantive_user_messages(messages: &[InterviewMessage]) -> Vec<&InterviewMessage> {
    messages
        .iter()
        .filter(|m| m.role == "user")
        .filter(|m| {
            !matches!(
                m.content.trim(),
                "不知道怎么回答" | "换一个问题" | "这个问题不想说" | "结束本次采访"
            )
        })
        .collect()
}

fn fallback_extraction(messages: &[InterviewMessage]) -> ExtractedStory {
    let user = substantive_user_messages(messages);
    let narrative = user
        .iter()
        .map(|m| m.content.trim())
        .collect::<Vec<_>>()
        .join("\n\n");
    let first = user.first().map(|m| m.content.trim()).unwrap_or("一段回忆");
    let title: String = first.chars().take(18).collect();
    let summary: String = narrative.chars().take(240).collect();
    ExtractedStory {
        title: Some(if title.is_empty() {
            "一段回忆".into()
        } else {
            title
        }),
        summary: Some(summary.clone()),
        narrative: Some(narrative),
        life_stage: None,
        time_text: None,
        year_start: None,
        year_end: None,
        time_precision: Some("unknown".into()),
        location_text: None,
        people: Vec::new(),
        themes: Vec::new(),
        emotions: Vec::new(),
        missing_details: vec!["具体时间".into(), "相关人物或地点".into()],
        recommended_chapter: Some("人生转折".into()),
    }
}

fn sanitize_year(year: Option<i32>) -> Option<i32> {
    year.filter(|y| (1800..=Utc::now().year() + 1).contains(y))
}

fn normalize_precision(value: Option<String>) -> String {
    match value.as_deref().map(str::trim) {
        Some("exact") => "exact",
        Some("approximate") => "approximate",
        Some("range") => "range",
        _ => "unknown",
    }
    .to_string()
}

fn normalize_chapter_title(extracted: &ExtractedStory) -> &'static str {
    if let Some(title) = extracted.recommended_chapter.as_deref() {
        if let Some(found) = DEFAULT_CHAPTER_TITLES
            .iter()
            .find(|known| **known == title.trim())
        {
            return found;
        }
    }
    match extracted.life_stage.as_deref().unwrap_or_default() {
        s if s.contains("童年") => "童年与家庭",
        s if s.contains("求学") || s.contains("学生") => "求学经历",
        s if s.contains("青年") => "青年时代",
        s if s.contains("工作") => "工作与事业",
        s if s.contains("婚姻") => "婚姻与家庭",
        s if s.contains("子女") => "子女与家庭生活",
        s if s.contains("退休") || s.contains("晚年") => "退休与晚年",
        _ => "人生转折",
    }
}

fn transcript_for_model(messages: &[InterviewMessage]) -> String {
    messages
        .iter()
        .map(|m| {
            let who = if m.role == "assistant" {
                "采访者"
            } else {
                "讲述人"
            };
            let content: String = m.content.chars().take(1200).collect();
            format!("{who}：{content}")
        })
        .collect::<Vec<_>>()
        .join("\n")
        .chars()
        .take(10000)
        .collect()
}

pub async fn extract_story_from_session(
    state: &AppState,
    user_id: Uuid,
    session_id: Uuid,
) -> AppResult<StoryCard> {
    let session = get_session(&state.pool, user_id, session_id).await?;
    if let Some(existing) = find_story_by_session(&state.pool, user_id, session_id).await? {
        return Ok(existing);
    }

    let messages = list_messages(&state.pool, user_id, session_id).await?;
    if substantive_user_messages(&messages).is_empty() {
        return Err(AppError::BadRequest("请先讲一段回忆，再收进故事箱".into()));
    }
    let memoir = get_memoir(&state.pool, user_id, session.memoir_id).await?;
    let transcript = transcript_for_model(&messages);
    let chapter_names = DEFAULT_CHAPTER_TITLES.join("、");
    let model_messages = vec![
        ChatMessage {
            role: "system".into(),
            content: STORY_EXTRACT_PROMPT.into(),
        },
        ChatMessage {
            role: "user".into(),
            content: format!(
                "回忆录主人：{}\n可选章节：{}\n\n采访记录：\n{}",
                memoir.subject_name, chapter_names, transcript
            ),
        },
    ];
    let (client, enable_thinking) = {
        let runtime = state.llm_runtime.read().await;
        (runtime.client.clone(), runtime.enable_thinking)
    };
    let extracted = match client
        .complete_with(&model_messages, CompleteOptions::chapter(enable_thinking))
        .await
    {
        Ok(completion) => {
            let _ = crate::settings::record_usage(
                &state.pool,
                "story_extract",
                &completion.model,
                completion.prompt_tokens,
                completion.completion_tokens,
                completion.total_tokens,
                completion.latency_ms,
                parse_json_object(&completion.content).is_some(),
                None,
            )
            .await;
            parse_json_object(&completion.content).unwrap_or_else(|| fallback_extraction(&messages))
        }
        Err(error) => {
            tracing::warn!(%error, %session_id, "story extraction failed; using transcript fallback");
            fallback_extraction(&messages)
        }
    };

    persist_story(&state.pool, &session, &messages, extracted).await
}

async fn persist_story(
    pool: &PgPool,
    session: &crate::interviews::service::InterviewSession,
    messages: &[InterviewMessage],
    extracted: ExtractedStory,
) -> AppResult<StoryCard> {
    let chapter_title = normalize_chapter_title(&extracted);
    let chapter_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM chapters WHERE memoir_id = $1 AND title = $2 LIMIT 1",
    )
    .bind(session.memoir_id)
    .bind(chapter_title)
    .fetch_optional(pool)
    .await?;

    let fallback = fallback_extraction(messages);
    let title = clean_optional(extracted.title)
        .or(fallback.title)
        .unwrap_or_else(|| "一段回忆".into());
    let summary = clean_optional(extracted.summary)
        .or(fallback.summary)
        .unwrap_or_default();
    let narrative = clean_optional(extracted.narrative)
        .or(fallback.narrative)
        .unwrap_or_else(|| summary.clone());
    let year_start = sanitize_year(extracted.year_start);
    let year_end = sanitize_year(extracted.year_end)
        .filter(|end| year_start.map(|start| *end >= start).unwrap_or(true));
    let people =
        serde_json::to_string(&clean_list(extracted.people)).unwrap_or_else(|_| "[]".into());
    let themes =
        serde_json::to_string(&clean_list(extracted.themes)).unwrap_or_else(|_| "[]".into());
    let emotions =
        serde_json::to_string(&clean_list(extracted.emotions)).unwrap_or_else(|_| "[]".into());
    let missing = serde_json::to_string(&clean_list(extracted.missing_details))
        .unwrap_or_else(|_| "[]".into());

    let mut tx = pool.begin().await?;
    let story_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO stories (
          memoir_id, session_id, title, summary, narrative, life_stage, time_text,
          year_start, year_end, time_precision, location_text, people, themes,
          emotions, missing_details, primary_chapter_id
        ) VALUES (
          $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
          $12::jsonb, $13::jsonb, $14::jsonb, $15::jsonb, $16
        )
        RETURNING id
        "#,
    )
    .bind(session.memoir_id)
    .bind(session.id)
    .bind(title)
    .bind(summary)
    .bind(narrative)
    .bind(clean_optional(extracted.life_stage))
    .bind(clean_optional(extracted.time_text))
    .bind(year_start)
    .bind(year_end)
    .bind(normalize_precision(extracted.time_precision))
    .bind(clean_optional(extracted.location_text))
    .bind(people)
    .bind(themes)
    .bind(emotions)
    .bind(missing)
    .bind(chapter_id)
    .fetch_one(&mut *tx)
    .await?;

    for message in messages {
        sqlx::query(
            "INSERT INTO story_sources (story_id, message_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(story_id)
        .bind(message.id)
        .execute(&mut *tx)
        .await?;
    }
    if let Some(chapter_id) = chapter_id {
        sqlx::query(
            r#"
            INSERT INTO story_chapter_relations (
              story_id, chapter_id, relation_type, relevance_score, classification_reason
            ) VALUES ($1, $2, 'primary', 1.0, '根据故事人生阶段与主题自动推荐')
            ON CONFLICT (story_id, chapter_id) DO NOTHING
            "#,
        )
        .bind(story_id)
        .bind(chapter_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    get_story(pool, session.memoir_id, story_id).await
}

const STORY_SELECT: &str = r#"
    SELECT s.id, s.memoir_id, s.session_id, s.title, s.summary, s.narrative,
           s.life_stage, s.time_text, s.year_start, s.year_end, s.time_precision,
           s.location_text, s.people::text AS people_json,
           s.themes::text AS themes_json, s.emotions::text AS emotions_json,
           s.missing_details::text AS missing_details_json,
           s.primary_chapter_id, c.title AS primary_chapter_title, s.status,
           (SELECT COUNT(*)::bigint FROM story_sources ss WHERE ss.story_id = s.id) AS source_count,
           s.created_at, s.updated_at
    FROM stories s
    LEFT JOIN chapters c ON c.id = s.primary_chapter_id
"#;

async fn get_story(pool: &PgPool, memoir_id: Uuid, story_id: Uuid) -> AppResult<StoryCard> {
    let sql = format!("{STORY_SELECT} WHERE s.id = $1 AND s.memoir_id = $2");
    let row = sqlx::query_as::<_, StoryRow>(&sql)
        .bind(story_id)
        .bind(memoir_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("story not found".into()))?;
    Ok(row.into())
}

async fn find_story_by_session(
    pool: &PgPool,
    user_id: Uuid,
    session_id: Uuid,
) -> AppResult<Option<StoryCard>> {
    let sql = format!(
        "{STORY_SELECT} JOIN memoirs m ON m.id = s.memoir_id \
         WHERE s.session_id = $1 AND (m.creator_user_id = $2 OR m.owner_user_id = $2)"
    );
    Ok(sqlx::query_as::<_, StoryRow>(&sql)
        .bind(session_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .map(Into::into))
}

pub async fn list_stories(
    pool: &PgPool,
    user_id: Uuid,
    memoir_id: Uuid,
) -> AppResult<Vec<StoryCard>> {
    let _ = get_memoir(pool, user_id, memoir_id).await?;
    let sql = format!(
        "{STORY_SELECT} WHERE s.memoir_id = $1 \
         ORDER BY s.year_start ASC NULLS LAST, s.created_at ASC"
    );
    Ok(sqlx::query_as::<_, StoryRow>(&sql)
        .bind(memoir_id)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(Into::into)
        .collect())
}

pub async fn confirm_story(pool: &PgPool, user_id: Uuid, story_id: Uuid) -> AppResult<StoryCard> {
    let memoir_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        UPDATE stories s
        SET status = 'confirmed', updated_at = NOW()
        FROM memoirs m
        WHERE s.id = $1 AND m.id = s.memoir_id
          AND (m.creator_user_id = $2 OR m.owner_user_id = $2)
        RETURNING s.memoir_id
        "#,
    )
    .bind(story_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("story not found".into()))?;
    get_story(pool, memoir_id, story_id).await
}

fn timeline_from(stories: &[StoryCard]) -> Vec<TimelineEvent> {
    stories
        .iter()
        .map(|story| TimelineEvent {
            story_id: story.id,
            title: story.title.clone(),
            time_text: story.time_text.clone(),
            year_start: story.year_start,
            year_end: story.year_end,
            time_precision: story.time_precision.clone(),
            summary: story.summary.clone(),
        })
        .collect()
}

fn story_material(stories: &[StoryCard]) -> String {
    stories
        .iter()
        .enumerate()
        .map(|(idx, story)| {
            format!(
                "故事{}《{}》\n时间：{}\n地点：{}\n内容：{}",
                idx + 1,
                story.title,
                story.time_text.as_deref().unwrap_or("时间未确定"),
                story.location_text.as_deref().unwrap_or("地点未确定"),
                story.narrative
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn fallback_chapter_from_stories(title: &str, stories: &[StoryCard]) -> String {
    format!(
        "【{title}】\n\n{}",
        stories
            .iter()
            .map(|story| story.narrative.trim())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    )
}

pub async fn organize_memoir(
    state: &AppState,
    user_id: Uuid,
    memoir_id: Uuid,
) -> AppResult<OrganizeResult> {
    let memoir = get_memoir(&state.pool, user_id, memoir_id).await?;
    let stories = list_stories(&state.pool, user_id, memoir_id).await?;
    let confirmed: Vec<StoryCard> = stories
        .into_iter()
        .filter(|story| story.status == "confirmed")
        .collect();
    if confirmed.is_empty() {
        return Err(AppError::BadRequest("请先确认至少一张故事卡片".into()));
    }

    let timeline = timeline_from(&confirmed);
    let mut groups: BTreeMap<(i32, Uuid, String), Vec<StoryCard>> = BTreeMap::new();
    for story in &confirmed {
        let Some(chapter_id) = story.primary_chapter_id else {
            continue;
        };
        let title = story
            .primary_chapter_title
            .clone()
            .unwrap_or_else(|| "人生转折".into());
        let sort_order = DEFAULT_CHAPTER_TITLES
            .iter()
            .position(|candidate| *candidate == title)
            .map(|idx| idx as i32)
            .unwrap_or(99);
        groups
            .entry((sort_order, chapter_id, title))
            .or_default()
            .push(story.clone());
    }

    let (client, enable_thinking) = {
        let runtime = state.llm_runtime.read().await;
        (runtime.client.clone(), runtime.enable_thinking)
    };
    let mut chapters = Vec::new();
    for ((_order, chapter_id, chapter_title), chapter_stories) in groups {
        let material = story_material(&chapter_stories);
        let model_messages = vec![
            ChatMessage {
                role: "system".into(),
                content: CHAPTER_FROM_STORIES_PROMPT.into(),
            },
            ChatMessage {
                role: "user".into(),
                content: format!(
                    "回忆录主人：{}\n章节：{}\n\n已确认故事卡片：\n{}",
                    memoir.subject_name, chapter_title, material
                ),
            },
        ];
        let content = match client
            .complete_with(&model_messages, CompleteOptions::chapter(enable_thinking))
            .await
        {
            Ok(completion)
                if !completion.used_fallback && !completion.content.trim().is_empty() =>
            {
                let _ = crate::settings::record_usage(
                    &state.pool,
                    "story_organize",
                    &completion.model,
                    completion.prompt_tokens,
                    completion.completion_tokens,
                    completion.total_tokens,
                    completion.latency_ms,
                    true,
                    None,
                )
                .await;
                completion.content
            }
            Ok(_) => fallback_chapter_from_stories(&chapter_title, &chapter_stories),
            Err(error) => {
                tracing::warn!(%error, %chapter_id, "chapter organization failed; using story text");
                fallback_chapter_from_stories(&chapter_title, &chapter_stories)
            }
        };
        let summary: String = content.chars().take(200).collect();
        let chapter = sqlx::query_as::<_, Chapter>(
            r#"
            UPDATE chapters
            SET content = $2, summary = $3, status = 'draft', updated_at = NOW()
            WHERE id = $1 AND memoir_id = $4
            RETURNING *
            "#,
        )
        .bind(chapter_id)
        .bind(content)
        .bind(summary)
        .bind(memoir_id)
        .fetch_one(&state.pool)
        .await?;
        sqlx::query(
            "UPDATE story_chapter_relations SET confirmed_by_user = TRUE WHERE chapter_id = $1 AND story_id = ANY($2)",
        )
        .bind(chapter_id)
        .bind(chapter_stories.iter().map(|story| story.id).collect::<Vec<_>>())
        .execute(&state.pool)
        .await?;
        chapters.push(chapter);
    }

    Ok(OrganizeResult {
        timeline,
        chapters,
        story_count: confirmed.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_json_from_markdown_fence() {
        let parsed =
            parse_json_object("```json\n{\"title\":\"雪天上学\",\"people\":[],\"themes\":[]}\n```")
                .expect("valid JSON object");
        assert_eq!(parsed.title.as_deref(), Some("雪天上学"));
    }

    #[test]
    fn rejects_invented_or_out_of_range_years() {
        assert_eq!(sanitize_year(Some(1200)), None);
        assert_eq!(sanitize_year(Some(2020)), Some(2020));
    }

    #[test]
    fn maps_life_stage_to_existing_chapter() {
        let extracted = ExtractedStory {
            title: None,
            summary: None,
            narrative: None,
            life_stage: Some("童年时期".into()),
            time_text: None,
            year_start: None,
            year_end: None,
            time_precision: None,
            location_text: None,
            people: vec![],
            themes: vec![],
            emotions: vec![],
            missing_details: vec![],
            recommended_chapter: None,
        };
        assert_eq!(normalize_chapter_title(&extracted), "童年与家庭");
    }
}
