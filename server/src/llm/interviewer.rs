use super::client::{ChatMessage, CompleteOptions, LlmClient, LlmCompletion};
use crate::interviews::service::InterviewMessage;

/// Oral-history interview craft (Studs Terkel / OHA-style): open-ended, one question,
/// follow the story, never invent, warm restraint for elders.
pub const INTERVIEWER_SYSTEM_PROMPT: &str = r#"你是一位资深口述史采访者与回忆录记者（风格参考：Studs Terkel 式倾听、口述历史协会 OHA 的开放式追问）。
你的工作是帮助老人把零散人生经历讲清楚，供后续写成回忆录章节。

【采访原则】
1. 一次只提一个开放式主问题；句子短、口语化、老人容易接得上。
2. 先倾听再追问：优先补全「时间、地点、在场人物、具体动作、当时感受」中尚未清楚的一项。
3. 用对方刚说过的原词轻轻接住，再往下问；不要审讯式连问，不要一次抛出多个问题。
4. 绝不虚构、不替对方编造人名/地名/情节；对方没说的细节不要当成事实去问。
5. 敏感话题（死亡、政治迫害、家暴、羞耻经历）只轻轻试探一次；对方回避则立刻换题并安抚。
6. 少做评价与说教；简短共情后立刻落到可回答的具体问题。
7. 若对方说「不知道 / 换一题 / 不想说」，温和换一个更轻、更具体的切口，不要责备。
8. 若一段故事要素已齐（时间+地点+人物+经过+感受），可自然收束或引向同主题下一件小事。

【输出格式】
- 只输出要对讲述人说的下一句（可含一句很短的回应 + 一个问题）。
- 不要输出 JSON、标题、分点、旁白或你的思考过程。"#;

pub async fn next_question(
    llm: &dyn LlmClient,
    topic: &str,
    subject_name: &str,
    history: &[InterviewMessage],
    user_content: &str,
) -> anyhow::Result<LlmCompletion> {
    let mut messages = Vec::new();
    messages.push(ChatMessage {
        role: "system".into(),
        content: format!(
            "{INTERVIEWER_SYSTEM_PROMPT}\n当前采访主题：{topic}\n回忆录主人：{subject_name}"
        ),
    });

    // Keep a short recent window so latency stays low as sessions grow.
    let start = history.len().saturating_sub(8);
    for m in &history[start..] {
        let role = match m.role.as_str() {
            "assistant" => "assistant",
            "system" => "system",
            _ => "user",
        };
        // Cap each turn so one long monologue cannot bloat the prompt.
        let content: String = m.content.chars().take(600).collect();
        messages.push(ChatMessage {
            role: role.into(),
            content,
        });
    }

    if history
        .last()
        .map(|m| m.content.as_str() != user_content || m.role != "user")
        .unwrap_or(true)
    {
        let content: String = user_content.chars().take(600).collect();
        messages.push(ChatMessage {
            role: "user".into(),
            content,
        });
    }

    // Same model; cap output + disable thinking to cut 20–50s CoT waste.
    llm.complete_with(&messages, CompleteOptions::interview())
        .await
}

#[cfg(test)]
mod tests {
    use super::INTERVIEWER_SYSTEM_PROMPT;

    #[test]
    fn interviewer_prompt_encodes_oral_history_craft() {
        let p = INTERVIEWER_SYSTEM_PROMPT;
        assert!(
            p.contains("一次只提一个") || p.contains("一个开放式"),
            "single open question"
        );
        assert!(
            p.contains("时间") && p.contains("地点") && p.contains("感受"),
            "follow-up on concrete detail"
        );
        assert!(
            p.contains("绝不虚构") || p.contains("不虚构") || p.contains("不要当成事实"),
            "no fabrication"
        );
        assert!(
            p.contains("口述") || p.contains("回忆录"),
            "memoir/oral history framing"
        );
    }
}
