//! Language models for the optional assistant and document review. The provider, model and key
//! come from Admin settings. Anthropic speaks its Messages API; everything else speaks the OpenAI
//! chat completions API. Tools are the user's own services, so the model can never do more than
//! the user could in the app.

use std::time::Duration;

use chrono::{Datelike, TimeZone, Utc};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgConnection;

use crate::context::Ctx;
use crate::error::{AppError, ErrorKind, Result};
use crate::settings::AiSettings;

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Text(String),
    ToolUse { id: String, name: String, input: Value },
    ToolResult { id: String, content: String, is_error: bool },
}

#[derive(Debug, Clone)]
pub struct Message {
    pub assistant: bool,
    pub blocks: Vec<Block>,
}

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Message { assistant: false, blocks: vec![Block::Text(text.into())] }
    }
}

pub struct Tool {
    pub name: String,
    pub description: String,
    pub schema: Value,
}

pub struct Reply {
    pub blocks: Vec<Block>,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

impl Reply {
    pub fn text(&self) -> String {
        self.blocks.iter().filter_map(|b| if let Block::Text(t) = b { Some(t.as_str()) } else { None }).collect::<Vec<_>>().join("\n")
    }
}

pub fn is_configured(settings: &AiSettings) -> bool {
    !settings.provider.is_empty() && !settings.model.is_empty()
}

fn base_url(settings: &AiSettings) -> Result<String> {
    let trimmed = settings.base_url.trim().trim_end_matches('/');
    Ok(match settings.provider.as_str() {
        "anthropic" if trimmed.is_empty() => "https://api.anthropic.com/v1".into(),
        "openai" if trimmed.is_empty() => "https://api.openai.com/v1".into(),
        "berget" => "https://api.berget.ai/v1".into(),
        "opper" => "https://api.opper.ai/v3/compat".into(),
        "litellm" if !trimmed.is_empty() => {
            if trimmed.ends_with("/v1") {
                trimmed.into()
            } else {
                format!("{trimmed}/v1")
            }
        }
        "anthropic" | "openai" | "openai-compatible" if !trimmed.is_empty() => trimmed.into(),
        _ => return Err(AppError::new(ErrorKind::Unavailable, "The AI provider is not set up. An Admin can do that in Settings, AI.")),
    })
}

/// Operator-run proxies may live on the private network; the public providers never do.
fn client<'a>(ctx: &'a Ctx, settings: &AiSettings) -> &'a reqwest::Client {
    match settings.provider.as_str() {
        "litellm" | "openai-compatible" => &ctx.app.http.internal,
        _ => &ctx.app.http.public,
    }
}

fn describe(status: u16, body: &str) -> String {
    match status {
        401 | 403 => format!("The AI provider refused the key (HTTP {status}). An Admin should check it in Settings, AI."),
        404 => "The AI provider does not know that model or address (HTTP 404).".into(),
        429 => "The AI provider is rate limiting or out of credit (HTTP 429). Try again shortly.".into(),
        _ => format!("The AI provider failed (HTTP {status}): {}", body.chars().take(200).collect::<String>()),
    }
}

fn authorize(request: reqwest::RequestBuilder, settings: &AiSettings) -> reqwest::RequestBuilder {
    let key = settings.api_key.as_deref().unwrap_or_default();
    if settings.provider == "anthropic" {
        request.header("x-api-key", key).header("anthropic-version", "2023-06-01")
    } else if key.is_empty() {
        request
    } else {
        request.bearer_auth(key)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Model {
    pub id: String,
    pub name: String,
}

/// The chat models the provider offers this key. A successful answer also proves the address and key work.
pub async fn list_models(ctx: &Ctx, settings: &AiSettings) -> Result<Vec<Model>> {
    let base = base_url(settings)?;
    let mut request = client(ctx, settings).get(format!("{base}/models")).timeout(Duration::from_secs(20));
    if settings.provider == "anthropic" {
        request = request.query(&[("limit", "1000")]);
    }
    let response = authorize(request, settings)
        .send()
        .await
        .map_err(|e| AppError::new(ErrorKind::Unavailable, "Could not reach the AI provider. Check the address.").with_cause(e))?;
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(AppError::new(ErrorKind::Unavailable, describe(status, &text)));
    }
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| AppError::new(ErrorKind::Unavailable, "The AI provider sent an unreadable model list").with_cause(e))?;
    Ok(parse_models(&value))
}

const NOT_CHAT: &[&str] =
    &["embed", "tts", "whisper", "dall-e", "moderation", "transcribe", "audio", "realtime", "image", "rerank", "babbage", "davinci"];

fn parse_models(value: &Value) -> Vec<Model> {
    let mut models: Vec<(i64, Model)> = value["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let id = m["id"].as_str()?.trim();
            let lower = id.to_ascii_lowercase();
            if id.is_empty() || id.len() > 200 || NOT_CHAT.iter().any(|n| lower.contains(n)) {
                return None;
            }
            let name = m["display_name"].as_str().or(m["name"].as_str()).filter(|n| !n.trim().is_empty()).unwrap_or(id);
            let created = m["created"]
                .as_i64()
                .or_else(|| m["created_at"].as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map(|t| t.timestamp()));
            Some((created.unwrap_or(0), Model { id: id.to_string(), name: name.chars().take(200).collect() }))
        })
        .collect();
    models.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
    models.dedup_by(|a, b| a.1.id == b.1.id);
    models.into_iter().take(1000).map(|(_, m)| m).collect()
}

fn anthropic_messages(messages: &[Message]) -> Vec<Value> {
    messages
        .iter()
        .map(|m| {
            let content: Vec<Value> = m
                .blocks
                .iter()
                .map(|b| match b {
                    Block::Text(t) => json!({ "type": "text", "text": t }),
                    Block::ToolUse { id, name, input } => json!({ "type": "tool_use", "id": id, "name": name, "input": input }),
                    Block::ToolResult { id, content, is_error } => {
                        json!({ "type": "tool_result", "tool_use_id": id, "content": content, "is_error": is_error })
                    }
                })
                .collect();
            json!({ "role": if m.assistant { "assistant" } else { "user" }, "content": content })
        })
        .collect()
}

fn openai_messages(system: &str, messages: &[Message]) -> Vec<Value> {
    let mut out = vec![json!({ "role": "system", "content": system })];
    for m in messages {
        if m.assistant {
            let text =
                m.blocks.iter().filter_map(|b| if let Block::Text(t) = b { Some(t.as_str()) } else { None }).collect::<Vec<_>>().join("\n");
            let calls: Vec<Value> = m
                .blocks
                .iter()
                .filter_map(|b| match b {
                    Block::ToolUse { id, name, input } => {
                        Some(json!({ "id": id, "type": "function", "function": { "name": name, "arguments": input.to_string() } }))
                    }
                    _ => None,
                })
                .collect();
            let mut msg = json!({ "role": "assistant", "content": if text.is_empty() { Value::Null } else { json!(text) } });
            if !calls.is_empty() {
                msg["tool_calls"] = json!(calls);
            }
            out.push(msg);
        } else {
            for b in &m.blocks {
                match b {
                    Block::Text(t) => out.push(json!({ "role": "user", "content": t })),
                    Block::ToolResult { id, content, .. } => out.push(json!({ "role": "tool", "tool_call_id": id, "content": content })),
                    Block::ToolUse { .. } => {}
                }
            }
        }
    }
    out
}

pub struct Prompt<'a> {
    /// What the tokens were spent on, for the usage table.
    pub kind: &'a str,
    pub system: &'a str,
    pub messages: &'a [Message],
    pub tools: &'a [Tool],
    pub max_tokens: u32,
}

/// One model call. Usage is recorded and counted against the monthly budget.
pub async fn complete(ctx: &Ctx, conn: &mut PgConnection, settings: &AiSettings, prompt: Prompt<'_>) -> Result<Reply> {
    let Prompt { kind, system, messages, tools, max_tokens } = prompt;
    if !is_configured(settings) {
        return Err(AppError::new(ErrorKind::Unavailable, "The AI features are off. An Admin can set up a provider in Settings, AI."));
    }
    check_budget(conn, settings).await?;
    let base = base_url(settings)?;
    let anthropic = settings.provider == "anthropic";
    let (url, body) = if anthropic {
        let tools: Vec<Value> =
            tools.iter().map(|t| json!({ "name": t.name, "description": t.description, "input_schema": t.schema })).collect();
        let mut body =
            json!({ "model": settings.model, "max_tokens": max_tokens, "system": system, "messages": anthropic_messages(messages) });
        if !tools.is_empty() {
            body["tools"] = json!(tools);
        }
        (format!("{base}/messages"), body)
    } else {
        let tools: Vec<Value> = tools
            .iter()
            .map(|t| json!({ "type": "function", "function": { "name": t.name, "description": t.description, "parameters": t.schema } }))
            .collect();
        let mut body = json!({ "model": settings.model, "max_tokens": max_tokens, "messages": openai_messages(system, messages) });
        if !tools.is_empty() {
            body["tools"] = json!(tools);
        }
        (format!("{base}/chat/completions"), body)
    };
    let request = authorize(client(ctx, settings).post(&url).timeout(Duration::from_secs(180)).json(&body), settings);
    let response = request.send().await.map_err(|e| {
        AppError::new(ErrorKind::Unavailable, "Could not reach the AI provider. Check the address in Settings, AI.").with_cause(e)
    })?;
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(AppError::new(ErrorKind::Unavailable, describe(status, &text)));
    }
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| AppError::new(ErrorKind::Unavailable, "The AI provider sent an unreadable reply").with_cause(e))?;
    let reply = if anthropic { parse_anthropic(&value) } else { parse_openai(&value) };
    sqlx::query("insert into ai_usage (user_id, kind, model, input_tokens, output_tokens) values ($1, $2, $3, $4, $5)")
        .bind(ctx.actor.as_ref().map(|a| a.user_id))
        .bind(kind)
        .bind(&settings.model)
        .bind(reply.input_tokens as i32)
        .bind(reply.output_tokens as i32)
        .execute(&mut *conn)
        .await?;
    Ok(reply)
}

fn parse_anthropic(value: &Value) -> Reply {
    let blocks = value["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|b| match b["type"].as_str() {
            Some("text") => Some(Block::Text(b["text"].as_str().unwrap_or_default().to_string())),
            Some("tool_use") => Some(Block::ToolUse {
                id: b["id"].as_str().unwrap_or_default().to_string(),
                name: b["name"].as_str().unwrap_or_default().to_string(),
                input: b["input"].clone(),
            }),
            _ => None,
        })
        .collect();
    Reply {
        blocks,
        input_tokens: value["usage"]["input_tokens"].as_i64().unwrap_or(0),
        output_tokens: value["usage"]["output_tokens"].as_i64().unwrap_or(0),
    }
}

fn parse_openai(value: &Value) -> Reply {
    let message = &value["choices"][0]["message"];
    let mut blocks = Vec::new();
    if let Some(text) = message["content"].as_str().filter(|t| !t.is_empty()) {
        blocks.push(Block::Text(text.to_string()));
    }
    for call in message["tool_calls"].as_array().into_iter().flatten() {
        let arguments = call["function"]["arguments"].as_str().unwrap_or("{}");
        blocks.push(Block::ToolUse {
            id: call["id"].as_str().unwrap_or_default().to_string(),
            name: call["function"]["name"].as_str().unwrap_or_default().to_string(),
            input: serde_json::from_str(arguments).unwrap_or_else(|_| json!({})),
        });
    }
    Reply {
        blocks,
        input_tokens: value["usage"]["prompt_tokens"].as_i64().unwrap_or(0),
        output_tokens: value["usage"]["completion_tokens"].as_i64().unwrap_or(0),
    }
}

pub async fn tokens_this_month(conn: &mut PgConnection) -> Result<i64> {
    let now = Utc::now();
    let start = Utc.with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0).single().unwrap_or(now);
    Ok(sqlx::query_scalar("select coalesce(sum(input_tokens + output_tokens), 0)::bigint from ai_usage where created_at >= $1")
        .bind(start)
        .fetch_one(&mut *conn)
        .await?)
}

async fn check_budget(conn: &mut PgConnection, settings: &AiSettings) -> Result<()> {
    if settings.monthly_token_budget > 0 && tokens_this_month(conn).await? as u64 >= settings.monthly_token_budget {
        return Err(AppError::new(ErrorKind::TooManyRequests, "This month's AI budget is used up. An Admin can raise it in Settings, AI."));
    }
    Ok(())
}

/// The first JSON object in a model's reply, which may wrap it in prose or a code fence.
pub fn json_in(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    serde_json::from_str(text.get(start..=end)?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_json_in_prose() {
        assert_eq!(json_in("Here you go:\n```json\n{\"a\": 1}\n```"), Some(json!({ "a": 1 })));
        assert_eq!(json_in("no json"), None);
    }

    #[test]
    fn openai_history_keeps_tool_turns() {
        let messages = vec![
            Message::user("hi"),
            Message { assistant: true, blocks: vec![Block::ToolUse { id: "c1".into(), name: "documents_list".into(), input: json!({}) }] },
            Message { assistant: false, blocks: vec![Block::ToolResult { id: "c1".into(), content: "[]".into(), is_error: false }] },
        ];
        let out = openai_messages("sys", &messages);
        assert_eq!(out[0]["role"], "system");
        assert_eq!(out[2]["tool_calls"][0]["function"]["name"], "documents_list");
        assert_eq!(out[3]["role"], "tool");
        assert_eq!(out[3]["tool_call_id"], "c1");
    }

    #[test]
    fn lists_chat_models_newest_first() {
        let models = parse_models(&json!({ "data": [
            { "id": "gpt-4o", "created": 1 },
            { "id": "text-embedding-3-small", "created": 3 },
            { "id": "gpt-5", "created": 2 },
            { "id": "claude-x", "display_name": "Claude X", "created_at": "2026-01-01T00:00:00Z" },
            { "id": "gpt-4o", "created": 1 },
        ] }));
        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["claude-x", "gpt-5", "gpt-4o"]);
        assert_eq!(models[0].name, "Claude X");
        assert!(parse_models(&json!({ "error": "nope" })).is_empty());
    }

    #[test]
    fn parses_both_reply_shapes() {
        let a = parse_anthropic(
            &json!({ "content": [{ "type": "text", "text": "ok" }, { "type": "tool_use", "id": "t", "name": "x", "input": { "a": 1 } }], "usage": { "input_tokens": 3, "output_tokens": 4 } }),
        );
        assert_eq!(a.blocks.len(), 2);
        assert_eq!(a.input_tokens + a.output_tokens, 7);
        let o = parse_openai(
            &json!({ "choices": [{ "message": { "content": null, "tool_calls": [{ "id": "c", "function": { "name": "x", "arguments": "{\"a\":1}" } }] } }], "usage": { "prompt_tokens": 1, "completion_tokens": 2 } }),
        );
        assert_eq!(o.blocks, vec![Block::ToolUse { id: "c".into(), name: "x".into(), input: json!({ "a": 1 }) }]);
    }
}
