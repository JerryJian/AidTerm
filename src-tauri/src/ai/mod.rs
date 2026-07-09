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

/// Build the OpenAI-compatible request body
fn build_request_body(messages: &[ChatMessage]) -> serde_json::Value {
    let api_messages: Vec<serde_json::Value> = messages.iter().map(|m| {
        let mut msg = serde_json::json!({
            "role": m.role,
            "content": m.content,
        });
        if let Some(ref id) = m.tool_call_id {
            msg["tool_call_id"] = serde_json::json!(id);
        }
        if let Some(ref calls) = m.tool_calls {
            msg["tool_calls"] = serde_json::to_value(calls).unwrap_or_default();
        }
        msg
    }).collect();

    serde_json::json!({
        "model": "", // will be filled by caller
        "messages": api_messages,
        "tools": [{
            "type": "function",
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
        }],
        "tool_choice": "auto"
    })
}

/// Parse the API response into our AiResponse
fn parse_response(body: &str) -> Result<AiResponse, String> {
    let v: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| format!("Failed to parse AI response: {}", e))?;

    let choice = v["choices"][0]
        .as_object()
        .ok_or("No choices in AI response")?;

    let message = choice["message"]
        .as_object()
        .ok_or("No message in AI response")?;

    let content = message.get("content").and_then(|c| c.as_str()).map(|s| s.to_string());

    let tool_calls = message.get("tool_calls")
        .and_then(|tc| tc.as_array())
        .map(|arr| {
            arr.iter().filter_map(|tc| {
                let id = tc["id"].as_str()?.to_string();
                let func = tc["function"].as_object()?;
                let name = func["name"].as_str()?.to_string();
                let args = func["arguments"].as_str()?.to_string();
                Some(ToolCall {
                    id,
                    function: ToolCallFunction { name, arguments: args },
                })
            }).collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(AiResponse {
        text: content,
        tool_calls,
    })
}

/// Fetch available models from the provider's API
async fn fetch_openai_models(
    base_url: &str,
    api_key: &str,
) -> Result<Vec<String>, String> {
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let request = client.get(&url);
    let request = if api_key.is_empty() {
        request
    } else {
        request.header("Authorization", format!("Bearer {}", api_key))
    };

    let response = request
        .send()
        .await
        .map_err(|e| format!("Failed to fetch models: {}", e))?;

    let status = response.status();
    let text = response.text().await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    if !status.is_success() {
        return Err(format!("API error ({}): {}", status, text));
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

async fn fetch_ollama_models(base_url: &str) -> Result<Vec<String>, String> {
    let url = format!("{}/api/tags", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch Ollama models: {}", e))?;

    let status = response.status();
    let text = response.text().await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    if !status.is_success() {
        return Err(format!("Ollama API error ({}): {}", status, text));
    }

    let v: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let models = v["models"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m["name"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    Ok(models)
}

async fn fetch_anthropic_models(base_url: &str, api_key: &str) -> Result<Vec<String>, String> {
    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let request = client.get(&url);
    let request = if api_key.is_empty() {
        request
    } else {
        request.header("x-api-key", api_key)
    };

    let response = request
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

/// Send a chat completion request to the AI API
pub async fn chat_completion(
    messages: Vec<ChatMessage>,
    config: &AiConfig,
) -> Result<AiResponse, String> {
    let mut body = build_request_body(&messages);
    body["model"] = serde_json::json!(config.model);

    let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("AI API request failed: {}", e))?;

    let status = response.status();
    let text = response.text().await
        .map_err(|e| format!("Failed to read AI response: {}", e))?;

    if !status.is_success() {
        return Err(format!("AI API error ({}): {}", status, text));
    }

    parse_response(&text)
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
