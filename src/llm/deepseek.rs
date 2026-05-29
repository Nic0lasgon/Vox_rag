use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use super::prompt::{self, parse_llm_response};
use super::provider::{LlmDecisionResponse, LlmProvider};

const MAX_RETRIES: u32 = 3;
const BASE_BACKOFF_MS: u64 = 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub function: FunctionCall,
    #[serde(rename = "type")]
    pub call_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Default)]
pub struct ChatOptions {
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub thinking: Option<ThinkingConfig>,
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThinkingConfig {
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct ChatResponse {
    pub choices: Vec<ChatChoice>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
pub struct ChatChoice {
    pub message: ChatMessageResponse,
}

#[derive(Debug, Deserialize)]
pub struct ChatMessageResponse {
    pub content: Option<String>,
    #[serde(default)]
    pub reasoning_content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Deserialize)]
pub struct Usage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    #[serde(default)]
    pub prompt_cache_hit_tokens: i64,
    #[serde(default)]
    pub prompt_cache_miss_tokens: i64,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<ThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolDefinition>>,
}

#[derive(Serialize)]
struct ResponseFormat {
    r#type: String,
}

#[derive(Serialize)]
struct FimRequest {
    model: String,
    prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    suffix: Option<String>,
    max_tokens: u32,
}

#[derive(Deserialize)]
struct FimResponse {
    choices: Vec<FimChoice>,
}

#[derive(Deserialize)]
struct FimChoice {
    text: String,
}

pub struct DeepSeekClient {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
}

impl DeepSeekClient {
    pub fn new(api_key: String, model: String, base_url: String, timeout_ms: u64) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .build()
            .context("Failed to create HTTP client for DeepSeek")?;
        Ok(Self {
            client,
            api_key,
            base_url,
            model,
        })
    }

    pub fn model_name(&self) -> &str {
        &self.model
    }

    fn build_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", self.api_key).parse().unwrap(),
        );
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        headers
    }

    async fn send_with_retry(
        &self,
        body: &impl Serialize,
        url: &str,
        max_retries: u32,
    ) -> Result<String> {
        let mut last_err = None;

        for attempt in 0..=max_retries {
            if attempt > 0 {
                let delay_ms = BASE_BACKOFF_MS * 2u64.pow(attempt - 1);
                warn!(
                    attempt,
                    delay_ms, url, "Retrying DeepSeek request after backoff"
                );
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }

            let response = self
                .client
                .post(url)
                .headers(self.build_headers())
                .json(body)
                .send()
                .await
                .context("Failed to send request to DeepSeek")?;

            let status = response.status();
            let text = response
                .text()
                .await
                .context("Failed to read DeepSeek response body")?;

            if status.is_success() {
                return Ok(text);
            }

            let code = status.as_u16();
            match code {
                429 | 500 | 503 => {
                    last_err = Some(format!("HTTP {}: {}", code, text));
                    if attempt < max_retries {
                        info!(
                            status = code,
                            attempt = attempt + 1,
                            max_retries,
                            "Retryable DeepSeek error"
                        );
                        continue;
                    }
                }
                _ => {
                    anyhow::bail!("DeepSeek API error ({}): {}", status, text);
                }
            }
        }

        anyhow::bail!(
            "DeepSeek request failed after {} retries: {}",
            max_retries,
            last_err.unwrap_or_default()
        )
    }

    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        options: ChatOptions,
    ) -> Result<ChatResponse> {
        let url = format!("{}/chat/completions", self.base_url);
        let request = ChatRequest {
            model: self.model.clone(),
            messages: messages.to_vec(),
            response_format: None,
            max_tokens: options.max_tokens,
            temperature: options.temperature,
            thinking: options.thinking,
            reasoning_effort: options.reasoning_effort,
            tools: None,
        };
        let body = self.send_with_retry(&request, &url, MAX_RETRIES).await?;
        serde_json::from_str(&body).context("Failed to parse DeepSeek chat response")
    }

    pub async fn chat_json(
        &self,
        messages: &[ChatMessage],
        options: ChatOptions,
    ) -> Result<ChatResponse> {
        let url = format!("{}/chat/completions", self.base_url);
        let request = ChatRequest {
            model: self.model.clone(),
            messages: messages.to_vec(),
            response_format: Some(ResponseFormat {
                r#type: "json_object".to_string(),
            }),
            max_tokens: options.max_tokens,
            temperature: options.temperature,
            thinking: options.thinking,
            reasoning_effort: options.reasoning_effort,
            tools: None,
        };
        let body = self.send_with_retry(&request, &url, MAX_RETRIES).await?;
        serde_json::from_str(&body).context("Failed to parse DeepSeek JSON response")
    }

    pub async fn chat_with_tools(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        options: ChatOptions,
    ) -> Result<ChatResponse> {
        let url = format!("{}/chat/completions", self.base_url);
        let request = ChatRequest {
            model: self.model.clone(),
            messages: messages.to_vec(),
            response_format: None,
            max_tokens: options.max_tokens,
            temperature: options.temperature,
            thinking: options.thinking,
            reasoning_effort: options.reasoning_effort,
            tools: Some(tools.to_vec()),
        };
        let body = self.send_with_retry(&request, &url, MAX_RETRIES).await?;
        serde_json::from_str(&body).context("Failed to parse DeepSeek tool response")
    }

    pub async fn chat_with_thinking(
        &self,
        messages: &[ChatMessage],
        effort: &str,
        options: ChatOptions,
    ) -> Result<ChatResponse> {
        let url = format!("{}/chat/completions", self.base_url);
        let request = ChatRequest {
            model: self.model.clone(),
            messages: messages.to_vec(),
            response_format: None,
            max_tokens: options.max_tokens,
            temperature: None,
            thinking: Some(ThinkingConfig { enabled: true }),
            reasoning_effort: Some(effort.to_string()),
            tools: None,
        };
        let body = self.send_with_retry(&request, &url, MAX_RETRIES).await?;
        serde_json::from_str(&body).context("Failed to parse DeepSeek thinking response")
    }

    pub async fn fim(&self, prompt: &str, suffix: Option<&str>, max_tokens: u32) -> Result<String> {
        let url = format!("{}/completions", self.base_url);
        let request = FimRequest {
            model: self.model.clone(),
            prompt: prompt.to_string(),
            suffix: suffix.map(|s| s.to_string()),
            max_tokens,
        };
        let body = self.send_with_retry(&request, &url, MAX_RETRIES).await?;
        let response: FimResponse =
            serde_json::from_str(&body).context("Failed to parse DeepSeek FIM response")?;
        response
            .choices
            .first()
            .map(|c| c.text.clone())
            .context("No choices in DeepSeek FIM response")
    }

    pub async fn chat_prefix(
        &self,
        messages: &[ChatMessage],
        prefix: &str,
        _stop: Option<&[&str]>,
    ) -> Result<ChatResponse> {
        let mut msgs = messages.to_vec();
        msgs.push(ChatMessage {
            role: "assistant".to_string(),
            content: prefix.to_string(),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            prefix: Some(true),
        });

        let url = format!("{}/chat/completions", self.base_url);
        let request = ChatRequest {
            model: self.model.clone(),
            messages: msgs,
            response_format: None,
            max_tokens: None,
            temperature: None,
            thinking: None,
            reasoning_effort: None,
            tools: None,
        };
        let body = self.send_with_retry(&request, &url, MAX_RETRIES).await?;
        serde_json::from_str(&body).context("Failed to parse DeepSeek prefix response")
    }

    pub fn estimate_cost(usage: &Usage) -> f64 {
        let cache_miss_cost = usage.prompt_cache_miss_tokens as f64 * 0.14 / 1_000_000.0;
        let cache_hit_cost = usage.prompt_cache_hit_tokens as f64 * 0.0028 / 1_000_000.0;
        let completion_cost = usage.completion_tokens as f64 * 0.28 / 1_000_000.0;
        cache_miss_cost + cache_hit_cost + completion_cost
    }
}

pub struct DeepSeekProvider {
    client: DeepSeekClient,
}

impl DeepSeekProvider {
    pub fn new(api_key: String, model: String, base_url: String, timeout_ms: u64) -> Result<Self> {
        let client = DeepSeekClient::new(api_key, model, base_url, timeout_ms)?;
        Ok(Self { client })
    }
}

#[async_trait]
impl LlmProvider for DeepSeekProvider {
    fn model_name(&self) -> &str {
        self.client.model_name()
    }

    async fn decide(&self, user_prompt: &str) -> Result<LlmDecisionResponse> {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: prompt::system_prompt().to_string(),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: None,
                prefix: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_prompt.to_string(),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: None,
                prefix: None,
            },
        ];

        let options = ChatOptions {
            max_tokens: Some(500),
            thinking: Some(ThinkingConfig { enabled: false }),
            ..Default::default()
        };

        let response = self.client.chat_json(&messages, options).await?;

        if let Some(ref usage) = response.usage {
            let cost = DeepSeekClient::estimate_cost(usage);
            info!(
                model = self.client.model_name(),
                prompt_tokens = usage.prompt_tokens,
                completion_tokens = usage.completion_tokens,
                cache_hit_tokens = usage.prompt_cache_hit_tokens,
                cache_miss_tokens = usage.prompt_cache_miss_tokens,
                cost_usd = format!("{:.6}", cost),
                "DeepSeek usage"
            );
        }

        let content = response
            .choices
            .first()
            .context("No choices in DeepSeek response")?
            .message
            .content
            .as_deref()
            .context("Empty content in DeepSeek response")?;

        parse_llm_response(content)
    }
}
