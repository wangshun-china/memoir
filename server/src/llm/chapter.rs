use super::client::{ChatMessage, CompleteOptions, LlmClient, LlmCompletion};
use crate::interviews::service::InterviewMessage;

/// Memoir chapter drafting craft: narrative-only-from-transcript, no invention
/// (editors like Frank McCourt-style voice from source material; fact fidelity first).
pub const CHAPTER_SYSTEM_PROMPT: &str = r#"你是一位严谨的回忆录作家与文字编辑。
任务：把口述采访记录整理成可阅读的章节正文草稿，供家人珍藏。

【写作原则 — 只根据素材，绝不演义】
1. 只能使用采访对话里出现的事实、人物、地点、时间与感受；不得虚构、补全未说出的情节或对话。
2. 写成连贯叙事（第一或第三人称择一并保持一致），不要保留「问：/答：」问答体。
3. 结构清晰：可用小标题分段；按时间或场景推进，删掉采访套话与重复。
4. 语言温暖、口语自然，适合老人与家属阅读；可轻度润色句子，但不得改变事实含义。
5. 材料不足时：只写已知内容，可注明「据现有口述整理，细节待补」，禁止用想象填空。
6. 保留讲述人独特的说法与情感色彩；不要写成新闻通稿或鸡汤说教。

【输出格式】
- 只输出章节正文（可含小标题）。
- 不要输出 JSON、写作说明、前言、后记或「以下是草稿」之类元话语。"#;

/// Max characters of transcript fed to the model (keeps generate latency bounded).
pub const CHAPTER_TRANSCRIPT_CHAR_CAP: usize = 8000;

/// Generate a chapter draft from interview messages.
pub async fn generate_chapter_draft(
    llm: &dyn LlmClient,
    topic: &str,
    subject_name: &str,
    history: &[InterviewMessage],
) -> anyhow::Result<LlmCompletion> {
    let transcript = format_transcript_capped(history, CHAPTER_TRANSCRIPT_CHAR_CAP);
    let messages = vec![
        ChatMessage {
            role: "system".into(),
            content: CHAPTER_SYSTEM_PROMPT.into(),
        },
        ChatMessage {
            role: "user".into(),
            content: format!(
                "回忆录主人：{subject_name}\n章节主题：{topic}\n\n采访对话记录：\n{transcript}\n\n请严格依据以上口述写出本章草稿（控制在合理篇幅，突出具体故事）。"
            ),
        },
    ];
    llm.complete_with(&messages, CompleteOptions::chapter())
        .await
}

fn format_transcript(history: &[InterviewMessage]) -> String {
    if history.is_empty() {
        return "（暂无对话）".into();
    }
    let mut lines = Vec::with_capacity(history.len());
    for m in history {
        let who = match m.role.as_str() {
            "assistant" => "采访者",
            "system" => "系统",
            _ => "讲述人",
        };
        lines.push(format!("{who}：{}", m.content.trim()));
    }
    lines.join("\n")
}

/// Prefer the most recent dialogue when the full transcript is long.
pub fn format_transcript_capped(history: &[InterviewMessage], max_chars: usize) -> String {
    let full = format_transcript(history);
    if full.chars().count() <= max_chars {
        return full;
    }
    // Take from the end so latest answers are kept.
    let mut acc = String::new();
    for line in full.lines().rev() {
        let candidate = if acc.is_empty() {
            line.to_string()
        } else {
            format!("{line}\n{acc}")
        };
        if candidate.chars().count() > max_chars {
            break;
        }
        acc = candidate;
    }
    if acc.is_empty() {
        // Single huge line: hard truncate head of full (keep tail).
        let tail: String = full
            .chars()
            .rev()
            .take(max_chars)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        format!("…（前文已省略）\n{tail}")
    } else {
        format!("…（前文已省略，保留最近对话）\n{acc}")
    }
}

/// Offline-friendly draft when LLM is unavailable: stitch user answers only.
pub fn fallback_chapter_draft(topic: &str, subject_name: &str, history: &[InterviewMessage]) -> String {
    let mut parts: Vec<String> = history
        .iter()
        .filter(|m| m.role == "user")
        .map(|m| m.content.trim().to_string())
        .filter(|s| !s.is_empty())
        .filter(|s| {
            !matches!(
                s.as_str(),
                "不知道怎么回答" | "换一个问题" | "这个问题不想说" | "结束本次采访"
            )
        })
        .collect();

    if parts.is_empty() {
        return format!(
            "【{topic}】\n\n关于{subject_name}的本章内容还在采访中，暂无足够素材生成正文。请继续对话后再试。"
        );
    }

    if parts.len() > 40 {
        parts.truncate(40);
    }
    format!(
        "【{topic}】\n\n以下根据{subject_name}的口述整理（自动草稿，待润色）：\n\n{}",
        parts.join("\n\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn msg(role: &str, content: &str) -> InterviewMessage {
        InterviewMessage {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            role: role.into(),
            content: content.into(),
            question_type: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn chapter_prompt_encodes_source_only_narrative() {
        let p = CHAPTER_SYSTEM_PROMPT;
        assert!(
            p.contains("不得虚构") || p.contains("绝不演义") || p.contains("只能使用"),
            "no fabrication from source only"
        );
        assert!(
            p.contains("连贯叙事") || p.contains("章节正文"),
            "coherent chapter narrative"
        );
        assert!(
            !p.contains("可以合理想象") && !p.contains("自由发挥"),
            "must not invite invention"
        );
        assert!(p.contains("不要输出 JSON") || p.contains("只输出章节正文"));
    }

    #[test]
    fn transcript_cap_keeps_recent_and_stays_under_limit() {
        let mut history = Vec::new();
        for i in 0..50 {
            history.push(msg("user", &format!("第{i}段很长的口述内容，重复填充一二三四五六七八九十。")));
            history.push(msg("assistant", &format!("那第{i}件事发生在哪里？")));
        }
        let capped = format_transcript_capped(&history, 500);
        assert!(capped.chars().count() <= 520, "len={}", capped.chars().count());
        assert!(capped.contains("前文已省略") || capped.contains("第49"));
    }
}
