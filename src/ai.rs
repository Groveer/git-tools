use anyhow::Result;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use crate::git::ConflictFile;
use crate::config::Settings;

pub struct ConflictResolver {
    client: Client,
    settings: Settings,
    #[cfg(test)]
    api_url: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize, Debug)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

impl ConflictResolver {
    pub fn new(settings: Settings) -> Self {
        Self {
            client: Client::new(),
            settings,
            #[cfg(test)]
            api_url: None,
        }
    }

    #[cfg(test)]
    pub fn with_api_url(settings: Settings, api_url: String) -> Self {
        Self {
            client: Client::new(),
            settings,
            api_url: Some(api_url),
        }
    }

    fn extract_conflict_content(content: &str) -> String {
        // 如果是大文件，只提取最相关的上下文
        const MAX_CONTEXT_LENGTH: usize = 500; // 提取的最大长度
        const CONTEXT_LINES: usize = 3; // 冲突附近要保留的上下文行数

        let lines: Vec<&str> = content.lines().collect();

        // 找到包含冲突标记的行
        let mut conflict_start = None;
        let mut conflict_end = None;

        for (i, line) in lines.iter().enumerate() {
            if line.contains("<<<<<<<") {
                conflict_start = Some(i);
            } else if line.contains(">>>>>>>") {
                conflict_end = Some(i);
            }
        }

        // 如果找不到冲突标记，返回截断的原始内容
        if conflict_start.is_none() || conflict_end.is_none() {
            return if content.len() > MAX_CONTEXT_LENGTH {
                format!("{}... (truncated)", &content[..MAX_CONTEXT_LENGTH])
            } else {
                content.to_string()
            };
        }

        // 计算要包含的行范围
        let start = conflict_start.unwrap().saturating_sub(CONTEXT_LINES);
        let end = (conflict_end.unwrap() + CONTEXT_LINES + 1).min(lines.len());

        // 提取冲突相关内容
        let relevant_lines: Vec<&str> = lines[start..end].to_vec();
        let result = relevant_lines.join("\n");

        // 如果提取的内容仍然太长，进行截断
        if result.len() > MAX_CONTEXT_LENGTH {
            format!("{}... (truncated)", &result[..MAX_CONTEXT_LENGTH])
        } else {
            result
        }
    }

    pub async fn resolve_conflict(&self, conflict: &ConflictFile) -> Result<String> {
        let system_prompt = "You are a Git merge conflict resolver. Analyze the conflict and choose the most appropriate resolution. Return ONLY the resolved content without any explanation.";

        // 精简冲突描述，减少发送的文本量
        // 提取 our_content 中的冲突内容
        let our_content = Self::extract_conflict_content(&conflict.our_content);
        let their_content = Self::extract_conflict_content(&conflict.their_content);
        let base_content = conflict.base_content.as_ref()
            .map(|content| Self::extract_conflict_content(content))
            .unwrap_or_default();

        let conflict_description = format!(
            "Resolve this Git merge conflict in {}. Here are the conflicting parts:\n\n\
            Our version: {}\n\n\
            Their version: {}\n\n\
            {}",
            conflict.path,
            our_content,
            their_content,
            if !base_content.is_empty() {
                format!("Base version: {}", base_content)
            } else {
                String::new()
            }
        );

        let request = ChatRequest {
            model: self.settings.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: conflict_description,
                },
            ],
            temperature: 0.7,
        };

        // 在测试环境中使用自定义 URL，否则使用 OpenAI 的 API URL
        #[cfg(test)]
        let url = if let Some(custom_url) = &self.api_url {
            custom_url.as_str()
        } else {
            "https://api.openai.com/v1/chat/completions"
        };
        #[cfg(not(test))]
        let url = "https://api.openai.com/v1/chat/completions";

        tracing::debug!("Request: {:?}", request);

        // 配置请求超时
        let timeout = std::time::Duration::from_secs(self.settings.timeout_seconds);

        // 添加重试逻辑
        let mut attempts = 0;
        let max_retries = self.settings.max_retries;

        while attempts <= max_retries {
            attempts += 1;
            tracing::info!("Attempt {}/{} to resolve conflict for file: {}",
                           attempts, max_retries + 1, conflict.path);

            match self.try_resolve(url, &request, timeout).await {
                Ok(resolution) => return Ok(resolution),
                Err(e) => {
                    if attempts > max_retries {
                        tracing::error!("Failed to get AI resolution after {} attempts: {}",
                                      attempts, e);
                        return Err(anyhow::anyhow!("Failed to get AI resolution after {} attempts: {}",
                                                attempts, e));
                    }

                    tracing::warn!("Attempt {} failed: {}. Retrying...", attempts, e);
                    // 重试前等待一段时间（指数退避）
                    tokio::time::sleep(std::time::Duration::from_millis(500 * 2u64.pow(attempts as u32))).await;
                }
            }
        }

        // 不应该到达这里，但为了编译通过
        Err(anyhow::anyhow!("Failed to get AI resolution"))
    }

    async fn try_resolve(&self, url: &str, request: &ChatRequest, timeout: std::time::Duration) -> Result<String> {
        let api_key = self.settings.openai_api_key.as_ref()
                   .ok_or_else(|| anyhow::anyhow!("OpenAI API key not set"))?;

        tracing::debug!("Sending request to OpenAI API: {}", url);

        let response = self.client
            .post(url)
            .timeout(timeout)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(request)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send request to OpenAI API: {}", e))?;

        // 检查响应状态
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await
                .unwrap_or_else(|_| String::from("Unable to get error details"));

            return Err(anyhow::anyhow!("API request failed with status {}: {}", status, error_text));
        }

        // 解析JSON响应
        let response_text = response.text().await
            .map_err(|e| anyhow::anyhow!("Failed to get response text: {}", e))?;

        tracing::debug!("OpenAI API response: {}", response_text);

        let chat_response: ChatResponse = serde_json::from_str(&response_text)
            .map_err(|e| anyhow::anyhow!("Failed to parse API response: {}, Response: {}", e, response_text))?;

        match chat_response.choices.first() {
            Some(choice) => Ok(choice.message.content.clone()),
            None => Err(anyhow::anyhow!("No resolution provided by AI")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;
    use serde_json::json;

    #[tokio::test]
    async fn test_resolve_conflict() -> Result<()> {
        // 设置模拟服务器
        let mut server = Server::new_async().await;

        // 获取模拟服务器地址
        let server_url = format!("http://{}/v1/chat/completions", server.host_with_port());

        // 模拟简化后的 OpenAI API 响应
        let mock_response = json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Resolved content"
                    }
                }
            ]
        });

        // 检查精简后的冲突描述
        let mock_server = server.mock("POST", "/v1/chat/completions")
            .match_header("content-type", "application/json")
            .with_status(200)
            .with_body(mock_response.to_string())
            .create_async().await;

        // 创建带有模拟设置的冲突解析器
        let mut settings = Settings::default();
        settings.openai_api_key = Some("test-key".to_string());
        settings.model = "gpt-3.5-turbo".to_string();

        // 创建一个测试冲突文件
        let conflict = ConflictFile {
            path: "test.txt".to_string(),
            our_content: "Our content".to_string(),
            their_content: "Their content".to_string(),
            base_content: Some("Base content".to_string()),
        };

        // 创建带有自定义客户端和 URL 的解析器
        let resolver = ConflictResolver::with_api_url(
            settings,
            server_url
        );

        // 模拟解析冲突
        let resolution = resolver.resolve_conflict(&conflict).await?;

        // 验证结果
        assert_eq!(resolution, "Resolved content");

        // 确保模拟服务器被调用
        mock_server.assert_async().await;

        Ok(())
    }

    // 测试没有基础版本的情况
    #[tokio::test]
    async fn test_resolve_conflict_without_base() -> Result<()> {
        // 设置模拟服务器
        let mut server = Server::new_async().await;

        // 获取模拟服务器地址
        let server_url = format!("http://{}/v1/chat/completions", server.host_with_port());

        // 模拟简化后的 OpenAI API 响应
        let mock_response = json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Resolved without base"
                    }
                }
            ]
        });

        // 检查没有基础版本的请求
        let mock_server = server.mock("POST", "/v1/chat/completions")
            .with_header("content-type", "application/json")
            .with_status(200)
            .with_body(mock_response.to_string())
            .create_async().await;

        // 创建带有模拟设置的冲突解析器
        let mut settings = Settings::default();
        settings.openai_api_key = Some("test-key".to_string());
        settings.model = "gpt-3.5-turbo".to_string();

        // 创建一个测试冲突文件，没有基础版本
        let conflict = ConflictFile {
            path: "test.txt".to_string(),
            our_content: "Our content".to_string(),
            their_content: "Their content".to_string(),
            base_content: None,
        };

        // 创建带有自定义客户端和 URL 的解析器
        let resolver = ConflictResolver::with_api_url(
            settings,
            server_url
        );

        // 模拟解析冲突
        let resolution = resolver.resolve_conflict(&conflict).await?;

        // 验证结果
        assert_eq!(resolution, "Resolved without base");

        // 确保模拟服务器被调用
        mock_server.assert_async().await;

        Ok(())
    }

    // 测试 OpenAI 返回空结果的错误情况
    #[tokio::test]
    async fn test_resolve_conflict_empty_response() -> Result<()> {
        // 设置模拟服务器
        let mut server = Server::new_async().await;

        // 模拟空结果响应
        let mock_response = json!({
            "choices": []
        });

        // 创建空结果测试用的模拟服务器
        let mock_server = server.mock("POST", "/v1/chat/completions")
            .expect(1)  // 设置期望只收到1次请求
            .with_header("content-type", "application/json")
            .with_status(200)
            .with_body(mock_response.to_string())
            .create_async().await;

        // 创建带有模拟设置的冲突解析器
        let mut settings = Settings::default();
        settings.openai_api_key = Some("test-key".to_string());
        settings.model = "gpt-3.5-turbo".to_string();
        settings.max_retries = 0; // 设置为0，禁用重试功能

        // 创建一个测试冲突文件
        let conflict = ConflictFile {
            path: "test.txt".to_string(),
            our_content: "Our content".to_string(),
            their_content: "Their content".to_string(),
            base_content: Some("Base content".to_string()),
        };

        // 创建带有自定义客户端和 URL 的解析器
        let resolver = ConflictResolver::with_api_url(
            settings,
            format!("http://{}/v1/chat/completions", server.host_with_port())
        );

        // 模拟解析冲突，应该返回错误
        let result = resolver.resolve_conflict(&conflict).await;

        // 验证结果是错误
        assert!(result.is_err());

        // 确保模拟服务器被调用
        mock_server.assert_async().await;

        Ok(())
    }

    // 测试 API 错误响应
    #[tokio::test]
    async fn test_api_error_response() -> Result<()> {
        // 设置模拟服务器
        let mut server = Server::new_async().await;

        // 模拟 API 错误
        let error_response = json!({
            "error": {
                "message": "Invalid API key",
                "type": "invalid_request_error",
                "code": "invalid_api_key"
            }
        });

        // 创建错误测试用的模拟服务器
        let mock_server = server.mock("POST", "/v1/chat/completions")
            .expect(1)  // 设置期望只收到1次请求
            .with_header("content-type", "application/json")
            .with_status(401)
            .with_body(error_response.to_string())
            .create_async().await;

        // 创建带有模拟设置的冲突解析器
        let mut settings = Settings::default();
        settings.openai_api_key = Some("invalid-key".to_string());
        settings.model = "gpt-3.5-turbo".to_string();
        settings.max_retries = 0; // 设置为0，禁用重试功能

        // 创建一个测试冲突文件
        let conflict = ConflictFile {
            path: "test.txt".to_string(),
            our_content: "Our content".to_string(),
            their_content: "Their content".to_string(),
            base_content: Some("Base content".to_string()),
        };

        // 创建带有自定义客户端和 URL 的解析器
        let resolver = ConflictResolver::with_api_url(
            settings,
            format!("http://{}/v1/chat/completions", server.host_with_port())
        );

        // 模拟解析冲突，应该返回错误
        let result = resolver.resolve_conflict(&conflict).await;

        // 验证结果是错误
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("API request failed with status 401"));

        // 确保模拟服务器被调用
        mock_server.assert_async().await;

        Ok(())
    }

}
