use crate::config::ModelCfg;
use crate::t;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn user(content: &str) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
        }
    }
    pub fn system(content: &str) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatOutcome {
    pub content: String,
    pub model: String,
    pub tokens_in: Option<u64>,
    pub tokens_out: Option<u64>,
    pub latency: Duration,
}

/// OpenAI-compatible 端点调用。api_key 由调用方从密钥存储解析后传入，
/// 本模块不落盘、不打日志。
pub fn chat(
    cfg: &ModelCfg,
    api_key: &str,
    messages: &[ChatMessage],
    json_mode: bool,
    timeout: Duration,
) -> Result<ChatOutcome> {
    let url = format!(
        "{}/chat/completions",
        cfg.base_url.trim().trim_end_matches('/')
    );
    let mut body = json!({
        "model": cfg.model,
        "messages": messages,
        "temperature": 0,
    });
    if json_mode {
        body["response_format"] = json!({ "type": "json_object" });
    }
    let agent = ureq::AgentBuilder::new().timeout(timeout).build();
    let t0 = Instant::now();
    let resp = agent
        .post(&url)
        .set("Authorization", &format!("Bearer {api_key}"))
        .send_json(body)
        .with_context(|| t!("model_req_failed", url))?;
    let value: serde_json::Value = resp.into_json().context(t!("model_not_json"))?;
    let content = value["choices"][0]["message"]["content"]
        .as_str()
        .context(t!("model_no_content"))?
        .to_string();
    Ok(ChatOutcome {
        model: value["model"]
            .as_str()
            .unwrap_or(cfg.model.as_str())
            .to_string(),
        tokens_in: value["usage"]["prompt_tokens"].as_u64(),
        tokens_out: value["usage"]["completion_tokens"].as_u64(),
        content,
        latency: t0.elapsed(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// 假 OpenAI-compatible 端点：读完请求体后回放固定响应。
    fn fake_endpoint(response: &'static str) -> (String, std::sync::mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(2) {
                let mut s = stream.unwrap();
                let mut buf = vec![0u8; 64 * 1024];
                let mut total = 0;
                loop {
                    let n = s.read(&mut buf[total..]).unwrap();
                    total += n;
                    if n == 0 {
                        break;
                    }
                    let head = String::from_utf8_lossy(&buf[..total]);
                    if let Some(pos) = head.find("\r\n\r\n") {
                        let len: usize = head[..pos]
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length: ")
                                    .and_then(|v| v.trim().parse().ok())
                            })
                            .unwrap_or(0);
                        if total >= pos + 4 + len {
                            break;
                        }
                    }
                }
                let _ = tx.send(String::from_utf8_lossy(&buf[..total]).to_string());
                let _ = s.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                        response.len()
                    )
                    .as_bytes(),
                );
                let _ = s.flush();
            }
        });
        (format!("http://{addr}/v1"), rx)
    }

    fn cfg(base_url: &str) -> ModelCfg {
        ModelCfg {
            base_url: base_url.into(),
            model: "m1".into(),
            key_ref: "keyring://t/t".into(),
            routing: Default::default(),
            price_in_per_mtok: None,
            price_out_per_mtok: None,
        }
    }

    #[test]
    fn chat_posts_openai_shape_and_parses_reply() {
        let (base, rx) = fake_endpoint(
            r#"{"choices":[{"message":{"role":"assistant","content":"pong"}}],"model":"m1-test","usage":{"prompt_tokens":7,"completion_tokens":2}}"#,
        );
        let out = chat(
            &cfg(&base),
            "sk-fake",
            &[ChatMessage::user("ping")],
            false,
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(out.content, "pong");
        assert_eq!(out.model, "m1-test");
        assert_eq!(out.tokens_in, Some(7));
        assert_eq!(out.tokens_out, Some(2));
        let req = rx.recv().unwrap();
        assert!(req.starts_with("POST /v1/chat/completions"));
        assert!(req.contains("Authorization: Bearer sk-fake"));
        assert!(req.contains("\"model\":\"m1\""));
        assert!(!req.contains("response_format"));
    }

    #[test]
    fn json_mode_sets_response_format() {
        let (base, rx) =
            fake_endpoint(r#"{"choices":[{"message":{"content":"{}"}}],"model":"m1"}"#);
        chat(
            &cfg(&base),
            "sk-fake",
            &[ChatMessage::user("json")],
            true,
            Duration::from_secs(5),
        )
        .unwrap();
        let req = rx.recv().unwrap();
        assert!(req.contains(r#""response_format":{"type":"json_object"}"#));
    }
}
