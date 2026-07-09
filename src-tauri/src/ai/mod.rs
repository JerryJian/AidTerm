use std::sync::Mutex;

pub struct AiState {
    pub active_chats: Mutex<std::collections::HashMap<String, Vec<ChatMessage>>>,
}

impl AiState {
    pub fn new() -> Self {
        Self { active_chats: Mutex::new(std::collections::HashMap::new()) }
    }

    pub fn save_history(&self, session_id: &str, messages: Vec<ChatMessage>) {
        if let Ok(mut chats) = self.active_chats.lock() {
            chats.insert(session_id.to_string(), messages);
        }
    }

    pub fn load_history(&self, session_id: &str) -> Vec<ChatMessage> {
        self.active_chats.lock()
            .ok()
            .and_then(|chats| chats.get(session_id).cloned())
            .unwrap_or_default()
    }

    pub fn clear_history(&self, session_id: &str) {
        if let Ok(mut chats) = self.active_chats.lock() {
            chats.remove(session_id);
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct AiConfig {
    pub provider: String,
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct ToolCall {
    pub id: String,
    pub function: ToolCallFunction,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct AiResponse {
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}

// --- OpenAI compatible (async-openai) ---

async fn chat_openai(messages: Vec<ChatMessage>, config: &AiConfig) -> Result<AiResponse, String> {
    use async_openai::Client;
    use async_openai::config::OpenAIConfig;
    use async_openai::types::chat::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
        ChatCompletionRequestUserMessage, ChatCompletionRequestAssistantMessage,
        ChatCompletionRequestToolMessage,
        ChatCompletionRequestSystemMessageContent, ChatCompletionRequestUserMessageContent,
        ChatCompletionRequestAssistantMessageContent, ChatCompletionRequestToolMessageContent,
        CreateChatCompletionRequestArgs, ChatCompletionTools, ChatCompletionTool,
        FunctionObject, ChatCompletionToolChoiceOption,
        ChatCompletionMessageToolCalls, ChatCompletionMessageToolCall,
        FunctionCall,
    };

    let openai_config = OpenAIConfig::new()
        .with_api_base(config.base_url.trim_end_matches('/'))
        .with_api_key(&config.api_key);
    let client = Client::with_config(openai_config);

    let mut api_messages: Vec<ChatCompletionRequestMessage> = Vec::new();
    for m in &messages {
        match m.role.as_str() {
            "system" => {
                api_messages.push(ChatCompletionRequestMessage::System(
                    ChatCompletionRequestSystemMessage {
                        content: ChatCompletionRequestSystemMessageContent::Text(m.content.clone()),
                        name: None,
                    },
                ));
            }
            "user" => {
                api_messages.push(ChatCompletionRequestMessage::User(
                    ChatCompletionRequestUserMessage {
                        content: ChatCompletionRequestUserMessageContent::Text(m.content.clone()),
                        name: None,
                    },
                ));
            }
            "assistant" => {
                let content = if m.content.is_empty() {
                    None
                } else {
                    Some(ChatCompletionRequestAssistantMessageContent::Text(m.content.clone()))
                };
                let tc = m.tool_calls.as_ref().map(|calls| {
                    calls.iter().map(|tc| {
                        ChatCompletionMessageToolCalls::Function(ChatCompletionMessageToolCall {
                            id: tc.id.clone(),
                            function: FunctionCall {
                                name: tc.function.name.clone(),
                                arguments: tc.function.arguments.clone(),
                            },
                        })
                    }).collect()
                });
                api_messages.push(ChatCompletionRequestMessage::Assistant(
                    ChatCompletionRequestAssistantMessage {
                        content,
                        refusal: None,
                        name: None,
                        audio: None,
                        tool_calls: tc,
                        #[allow(deprecated)]
                        function_call: None,
                    },
                ));
            }
            "tool" => {
                if let Some(ref id) = m.tool_call_id {
                    api_messages.push(ChatCompletionRequestMessage::Tool(
                        ChatCompletionRequestToolMessage {
                            content: ChatCompletionRequestToolMessageContent::Text(m.content.clone()),
                            tool_call_id: id.clone(),
                        },
                    ));
                }
            }
            _ => {}
        }
    }

    let tool = ChatCompletionTools::Function(ChatCompletionTool {
        function: FunctionObject {
            name: "execute_command".to_string(),
            description: Some("在服务器上执行一条 shell 命令，返回命令的输出结果。执行命令后，系统会将输出结果返回给你，请根据结果继续推理。".to_string()),
            parameters: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "要执行的 shell 命令"
                    }
                },
                "required": ["command"]
            })),
            strict: None,
        },
    });

    let request = CreateChatCompletionRequestArgs::default()
        .model(&config.model)
        .messages(api_messages)
        .tools(vec![tool])
        .tool_choice(ChatCompletionToolChoiceOption::Mode(
            async_openai::types::chat::ToolChoiceOptions::Auto,
        ))
        .build()
        .map_err(|e| format!("Failed to build request: {}", e))?;

    let response = client.chat().create(request)
        .await
        .map_err(|e| format!("OpenAI API error: {}", e))?;

    let choice = response.choices.into_iter().next()
        .ok_or("No choices in AI response")?;

    let text = choice.message.content;
    let tool_calls = choice.message.tool_calls.unwrap_or_default()
        .into_iter()
        .filter_map(|tc| match tc {
            ChatCompletionMessageToolCalls::Function(f) => Some(ToolCall {
                id: f.id,
                function: ToolCallFunction {
                    name: f.function.name,
                    arguments: f.function.arguments,
                },
            }),
            _ => None,
        })
        .collect();

    Ok(AiResponse { text, tool_calls })
}

async fn fetch_openai_models(base_url: &str, api_key: &str) -> Result<Vec<String>, String> {
    use async_openai::Client;
    use async_openai::config::OpenAIConfig;

    let openai_config = OpenAIConfig::new()
        .with_api_base(base_url.trim_end_matches('/'))
        .with_api_key(api_key);
    let client = Client::with_config(openai_config);

    let response = client.models().list()
        .await
        .map_err(|e| format!("Failed to fetch models: {}", e))?;

    Ok(response.data.into_iter().map(|m| m.id).collect())
}

// --- Ollama (ollama-rs) ---

async fn chat_ollama(messages: Vec<ChatMessage>, config: &AiConfig) -> Result<AiResponse, String> {
    use ollama_rs::Ollama;
    use ollama_rs::generation::chat::{ChatMessage as OllamaChatMessage, MessageRole};
    use ollama_rs::generation::chat::request::ChatMessageRequest;

    let ollama = Ollama::builder()
        .url(config.base_url.trim_end_matches('/'))
        .build();

    let history: Vec<OllamaChatMessage> = messages.iter().map(|m| {
        let role = match m.role.as_str() {
            "user" => MessageRole::User,
            "assistant" => MessageRole::Assistant,
            "system" => MessageRole::System,
            "tool" => MessageRole::Tool,
            _ => MessageRole::User,
        };
        OllamaChatMessage::new(role, m.content.clone())
    }).collect();

    let request = ChatMessageRequest::new(config.model.clone(), history);

    let response = ollama.send_chat_messages(request)
        .await
        .map_err(|e| format!("Ollama API error: {}", e))?;

    let text = response.message.content;

    Ok(AiResponse { text: Some(text), tool_calls: vec![] })
}

async fn fetch_ollama_models(base_url: &str) -> Result<Vec<String>, String> {
    use ollama_rs::Ollama;

    let ollama = Ollama::builder()
        .url(base_url.trim_end_matches('/'))
        .build();

    let models = ollama.list_local_models()
        .await
        .map_err(|e| format!("Failed to fetch Ollama models: {}", e))?;

    Ok(models.into_iter().map(|m| m.name).collect())
}

// --- Anthropic (anthropic-ai-sdk) ---

async fn chat_anthropic(messages: Vec<ChatMessage>, config: &AiConfig) -> Result<AiResponse, String> {
    use anthropic_ai_sdk::client::AnthropicClient;
    use anthropic_ai_sdk::types::message::{
        CreateMessageParams, Message as AnthropicMessage, RequiredMessageParams,
        Role, ContentBlock, MessageClient,
    };

    let client = AnthropicClient::new::<anthropic_ai_sdk::types::model::ModelError>(
        &config.api_key, "2023-06-01",
    )
    .map_err(|e| format!("Failed to create Anthropic client: {}", e))?;

    let mut anthropic_messages: Vec<AnthropicMessage> = Vec::new();
    for m in &messages {
        let role = match m.role.as_str() {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            _ => continue,
        };
        anthropic_messages.push(AnthropicMessage::new_text(role, m.content.clone()));
    }

    let params = CreateMessageParams::new(RequiredMessageParams {
        model: config.model.clone(),
        messages: anthropic_messages,
        max_tokens: 4096,
    });

    let response = client.create_message(Some(&params))
        .await
        .map_err(|e| format!("Anthropic API error: {}", e))?;

    let mut text = String::new();
    for block in response.content {
        if let ContentBlock::Text { text: t } = block {
            text.push_str(&t);
        }
    }

    Ok(AiResponse { text: Some(text), tool_calls: vec![] })
}

async fn fetch_anthropic_models(base_url: &str, api_key: &str) -> Result<Vec<String>, String> {
    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(&url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch Anthropic models: {}", e))?;

    let status = response.status();
    let text = response.text().await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    if !status.is_success() {
        return Err(format!("Anthropic API error ({}): {}", status, text));
    }

    let v: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let models = v["data"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m["id"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    Ok(models)
}

// --- Public API ---

pub async fn fetch_models(
    provider: &str,
    base_url: &str,
    api_key: &str,
) -> Result<Vec<String>, String> {
    match provider {
        "ollama" => fetch_ollama_models(base_url).await,
        "anthropic" => fetch_anthropic_models(base_url, api_key).await,
        _ => fetch_openai_models(base_url, api_key).await,
    }
}

pub async fn chat_completion(
    messages: Vec<ChatMessage>,
    config: &AiConfig,
) -> Result<AiResponse, String> {
    match config.provider.as_str() {
        "ollama" => chat_ollama(messages, config).await,
        "anthropic" => chat_anthropic(messages, config).await,
        _ => chat_openai(messages, config).await,
    }
}

/// Execute a shell command and return its output
pub async fn execute_command(command: &str) -> Result<String, String> {
    let output = if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/C", command])
            .output()
            .map_err(|e| format!("Failed to execute command: {}", e))?
    } else {
        std::process::Command::new("sh")
            .args(["-c", command])
            .output()
            .map_err(|e| format!("Failed to execute command: {}", e))?
    };

    let mut result = String::new();

    if !output.stdout.is_empty() {
        result.push_str(&String::from_utf8_lossy(&output.stdout));
    }

    if !output.stderr.is_empty() {
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(&String::from_utf8_lossy(&output.stderr));
    }

    if !output.status.success() {
        let exit_code = output.status.code().unwrap_or(-1);
        result.push_str(&format!("\n[退出码: {}]", exit_code));
    }

    Ok(result)
}
