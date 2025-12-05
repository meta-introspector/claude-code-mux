use axum::{response::{IntoResponse, Response}, Json};
use hyper::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{info, warn};

use super::error::AppError;
use crate::models::{AnthropicRequest, Message, MessageContent, SystemPrompt, Tool, Usage};


// Temporarily define AnthropicResponse and AnthropicResponseMessage here
// These should ideally be in src/models/mod.rs or a specific provider module
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AnthropicResponse {
    pub id: String,
    pub r#type: String,
    pub role: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    pub content: Vec<MessageContent>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AnthropicResponseMessage {
    pub r#type: String,
    pub text: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenAIRequest {
    pub model: String,
    pub messages: Vec<OpenAIMessage>,
    #[serde(default)]
    pub stream: bool,
    // Other fields can be added as needed
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenAIMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OpenAIResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<OpenAIChoice>,
    pub usage: OpenAIUsage,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OpenAIChoice {
    pub index: u32,
    pub message: OpenAIMessage,
    pub finish_reason: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OpenAIUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

pub fn transform_openai_to_anthropic(
    openai_request: OpenAIRequest,
) -> Result<AnthropicRequest, String> {
    let mut anthropic_messages: Vec<Message> = Vec::new();
    let mut system_prompt: Option<String> = None;

    for msg in openai_request.messages {
        match msg.role.as_str() {
            "system" => {
                // OpenAI has a system message role, Anthropic uses a system prompt string
                if system_prompt.is_some() {
                    return Err("Multiple system messages found in OpenAI request".to_string());
                }
                system_prompt = Some(msg.content);
            }
            "user" => anthropic_messages.push(Message {
                role: "user".to_string(), // Use string role
                content: MessageContent::Text(msg.content),
            }),
            "assistant" => anthropic_messages.push(Message {
                role: "assistant".to_string(), // Use string role
                content: MessageContent::Text(msg.content),
            }),
            _ => return Err(format!("Unsupported OpenAI message role: {}", msg.role)),
        }
    }

    Ok(AnthropicRequest {
        model: openai_request.model,
        messages: anthropic_messages,
        system: system_prompt.map(|s| SystemPrompt::Text(s)), // Convert Option<String> to Option<SystemPrompt>
        stream: Some(openai_request.stream),
        // Map other fields as needed, or leave as default/None
        max_tokens: 4096, // Default for now, should be configurable or mapped
        temperature: None,
        top_p: None,
        top_k: None,
        stop_sequences: None,
        tools: None,
        thinking: None,
        metadata: None,
    })
}

pub fn transform_anthropic_to_openai(
    anthropic_response: AnthropicResponse,
    original_model: String,
) -> OpenAIResponse {
    let choices = anthropic_response
        .content
        .into_iter()
        .filter_map(|msg_content| {
            if let MessageContent::Text(text) = msg_content { // Use MessageContent
                Some(OpenAIChoice {
                    index: 0, // Anthropic response is typically a single choice
                    message: OpenAIMessage {
                        role: "assistant".to_string(), // Anthropic always responds as assistant
                        content: text,
                    },
                    finish_reason: anthropic_response
                        .stop_reason
                        .unwrap_or("stop".to_string()),
                })
            } else {
                warn!("Anthropic response contained non-text content, skipping for OpenAI conversion.");
                None
            }
        })
        .collect();

    OpenAIResponse {
        id: anthropic_response.id,
        object: "chat.completion".to_string(),
        created: anthropic_response.stop_sequence.map_or(0, |_| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        }),
        model: original_model,
        choices,
        usage: OpenAIUsage {
            prompt_tokens: anthropic_response
                .usage
                .as_ref()
                .map_or(0, |u| u.input_tokens),
            completion_tokens: anthropic_response
                .usage
                .as_ref()
                .map_or(0, |u| u.output_tokens),
            total_tokens: anthropic_response
                .usage
                .as_ref()
                .map_or(0, |u| u.input_tokens + u.output_tokens),
        },
    }
}

// Handler for /v1/models
pub async fn open_ai_compat_models() -> impl IntoResponse {
    Json(json!({
        "object": "list",
        "data": [
            {
                "id": "gpt-4",
                "object": "model",
                "created": 1677649551,
                "owned_by": "openai",
            },
            {
                "id": "gpt-3.5-turbo",
                "object": "model",
                "created": 1677649551,
                "owned_by": "openai",
            },
            // Add other supported models here
        ]
    }))
}

pub async fn open_ai_compat_completions() -> Result<Response, AppError> {
    Ok((
        StatusCode::NOT_IMPLEMENTED,
        "The /v1/completions endpoint is not yet supported in this OpenAI compatibility layer. Please use /v1/chat/completions.",
    )
        .into_response())
}