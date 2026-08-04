use std::sync::Mutex;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

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

    let page_tool = ChatCompletionTools::Function(ChatCompletionTool {
        function: FunctionObject {
            name: "read_output_page".to_string(),
            description: Some("读取命令输出中指定的一页内容。当工具结果注明输出共有 N 页时使用。参数 output_id 为命令输出的唯一标识，page 为页码（从 1 开始）。".to_string()),
            parameters: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "output_id": {
                        "type": "string",
                        "description": "命令输出的唯一标识"
                    },
                    "page": {
                        "type": "integer",
                        "description": "页码（从 1 开始）"
                    }
                },
                "required": ["output_id", "page"]
            })),
            strict: None,
        },
    });

    let request = CreateChatCompletionRequestArgs::default()
        .model(&config.model)
        .messages(api_messages)
        .tools(vec![tool, page_tool])
        .tool_choice(ChatCompletionToolChoiceOption::Mode(
            async_openai::types::chat::ToolChoiceOptions::Auto,
        ))
        .parallel_tool_calls(false)
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
    use ollama_rs::generation::chat::{ChatMessage as OllamaChatMessage, MessageRole, request::ChatMessageRequest};
    use ollama_rs::generation::tools::ToolInfo;

    let url = config.base_url.trim_end_matches('/');
    let ollama = Ollama::try_new(url)
        .map_err(|e| format!("Failed to create Ollama client: {}", e))?;

    let ollama_messages: Vec<OllamaChatMessage> = messages.iter().map(|m| {
        let role = match m.role.as_str() {
            "system" => MessageRole::System,
            "user" => MessageRole::User,
            "assistant" => MessageRole::Assistant,
            "tool" => MessageRole::Tool,
            _ => MessageRole::User,
        };
        let mut msg = OllamaChatMessage::new(role, m.content.clone());
        if let Some(ref tool_calls) = m.tool_calls {
            msg.tool_calls = tool_calls.iter().map(|tc| {
                ollama_rs::generation::tools::ToolCall {
                    function: ollama_rs::generation::tools::ToolCallFunction {
                        name: tc.function.name.clone(),
                        arguments: serde_json::from_str::<serde_json::Value>(&tc.function.arguments)
                            .unwrap_or(serde_json::json!({})),
                    },
                }
            }).collect();
        }
        msg
    }).collect();

    let tool_info: Vec<ToolInfo> = vec![
        serde_json::from_value(serde_json::json!({
            "type": "Function",
            "function": {
                "name": "execute_command",
                "description": "在服务器上执行一条 shell 命令，返回命令的输出结果。执行命令后，系统会将输出结果返回给你，请根据结果继续推理。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "要执行的 shell 命令"
                        }
                    },
                    "required": ["command"]
                }
            }
        })).map_err(|e| format!("Failed to build tool info: {}", e))?,
        serde_json::from_value(serde_json::json!({
            "type": "Function",
            "function": {
                "name": "read_output_page",
                "description": "读取命令输出中指定的一页内容。当工具结果注明输出共有 N 页时使用。参数 output_id 为命令输出的唯一标识，page 为页码（从 1 开始）。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "output_id": {
                            "type": "string",
                            "description": "命令输出的唯一标识"
                        },
                        "page": {
                            "type": "integer",
                            "description": "页码（从 1 开始）"
                        }
                    },
                    "required": ["output_id", "page"]
                }
            }
        })).map_err(|e| format!("Failed to build tool info: {}", e))?,
    ];

    let request = ChatMessageRequest::new(config.model.clone(), ollama_messages)
        .tools(tool_info);

    let response = ollama.send_chat_messages(request)
        .await
        .map_err(|e| format!("Ollama API error: {}", e))?;

    let text = if response.message.content.is_empty() {
        None
    } else {
        Some(response.message.content)
    };

    let tool_calls = response.message.tool_calls
        .into_iter()
        .map(|tc| ToolCall {
            id: format!("call_{}", uuid::Uuid::new_v4()),
            function: ToolCallFunction {
                name: tc.function.name,
                arguments: tc.function.arguments.to_string(),
            },
        })
        .collect();

    Ok(AiResponse { text, tool_calls })
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

// --- Public API ---

pub async fn fetch_models(
    provider: &str,
    base_url: &str,
    api_key: &str,
) -> Result<Vec<String>, String> {
    match provider {
        "ollama" => fetch_ollama_models(base_url).await,
        _ => fetch_openai_models(base_url, api_key).await,
    }
}

pub async fn chat_completion(
    messages: Vec<ChatMessage>,
    config: &AiConfig,
) -> Result<AiResponse, String> {
    match config.provider.as_str() {
        "ollama" => chat_ollama(messages, config).await,
        _ => chat_openai(messages, config).await,
    }
}

/// Execute a shell command and return its output
pub async fn execute_command(command: &str) -> Result<String, String> {
    let output = if cfg!(target_os = "windows") {
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("cmd")
                .args(["/C", command])
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .output()
                .map_err(|e| format!("Failed to execute command: {}", e))?
        }
        #[cfg(not(target_os = "windows"))]
        {
            unreachable!()
        }
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
