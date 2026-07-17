use super::client::{ChatMessage, LlmClient, LlmCompletion};
use crate::interviews::service::InterviewMessage;

const SYSTEM_PROMPT: &str = r#"你是一位温和、耐心的人生回忆录采访者。
规则：
1. 一次只问一个主要问题。
2. 问题要短、具体、容易回答，优先追问时间、场景、人物、行动、感受。
3. 不要虚构用户没有说过的细节。
4. 对敏感话题不连续逼问。
5. 只输出下一句要问用户的问题或简短回应+问题，不要输出 JSON 或解释。"#;

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
        content: format!("{SYSTEM_PROMPT}\n当前采访主题：{topic}\n回忆录主人：{subject_name}"),
    });

    let start = history.len().saturating_sub(12);
    for m in &history[start..] {
        let role = match m.role.as_str() {
            "assistant" => "assistant",
            "system" => "system",
            _ => "user",
        };
        messages.push(ChatMessage {
            role: role.into(),
            content: m.content.clone(),
        });
    }

    if history
        .last()
        .map(|m| m.content.as_str() != user_content || m.role != "user")
        .unwrap_or(true)
    {
        messages.push(ChatMessage {
            role: "user".into(),
            content: user_content.to_string(),
        });
    }

    llm.complete(&messages).await
}
