use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 模型供应商目录条目。base_url 为 OpenAI-compatible 根（不含 /chat/completions）。
/// 接入点不写死：内置值只是官方文档当前地址，运行时可被
/// ~/.icewright/providers.json 覆盖/扩充（自部署网关、私有化端点均走这里），
/// 且 list_models 会实时访问端点确认可用性与最新模型名。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provider {
    pub name: String,
    pub display: String,
    pub base_url: String,
    pub docs_url: String,
    pub default_model: String,
    /// 密钥环境变量候选（按序取第一个已设置的）；用户自配环境变量的场景
    #[serde(default)]
    pub key_envs: Vec<String>,
}

fn builtin() -> Vec<Provider> {
    let p = |name: &str,
             display: &str,
             base_url: &str,
             docs_url: &str,
             default_model: &str,
             key_envs: &[&str]| Provider {
        name: name.into(),
        display: display.into(),
        base_url: base_url.into(),
        docs_url: docs_url.into(),
        default_model: default_model.into(),
        key_envs: key_envs.iter().map(|s| s.to_string()).collect(),
    };
    vec![
        p(
            "deepseek",
            "DeepSeek 深度求索",
            "https://api.deepseek.com/v1",
            "https://api-docs.deepseek.com/",
            "deepseek-chat",
            &["DEEPSEEK_API_KEY", "OPENAI_API_KEY"],
        ),
        p(
            "openai",
            "OpenAI",
            "https://api.openai.com/v1",
            "https://platform.openai.com/docs/api-reference",
            "gpt-4o-mini",
            &["OPENAI_API_KEY"],
        ),
        p(
            "moonshot",
            "Moonshot 月之暗面（Kimi）",
            "https://api.moonshot.cn/v1",
            "https://platform.moonshot.cn/docs/api/chat",
            "moonshot-v1-8k",
            &["MOONSHOT_API_KEY", "OPENAI_API_KEY"],
        ),
        p(
            "zhipu",
            "智谱 GLM",
            "https://open.bigmodel.cn/api/paas/v4",
            "https://docs.bigmodel.cn/cn/guide/develop/openai/overview",
            "glm-4-plus",
            &["ZHIPUAI_API_KEY", "ZAI_API_KEY", "OPENAI_API_KEY"],
        ),
        p(
            "dashscope",
            "阿里云百炼（Qwen，OpenAI compatible-mode）",
            "https://dashscope.aliyuncs.com/compatible-mode/v1",
            "https://help.aliyun.com/zh/model-studio/developer-reference/compatibility-of-openai-with-dashscope",
            "qwen-plus",
            &["DASHSCOPE_API_KEY", "QWEN_API_KEY", "OPENAI_API_KEY"],
        ),
        p(
            "ollama",
            "Ollama 本地自部署",
            "http://127.0.0.1:11434/v1",
            "https://github.com/ollama/ollama/blob/main/docs/openai.md",
            "llama3.1",
            &["OLLAMA_API_KEY"],
        ),
    ]
}

pub fn overrides_path() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .context("无法确定用户主目录")?;
    Ok(PathBuf::from(home)
        .join(".icewright")
        .join("providers.json"))
}

fn load_overrides_file(path: &Path) -> Result<Vec<Provider>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("无法读取供应商覆盖文件 {}", path.display()))?;
    parse_overrides(&raw).with_context(|| format!("供应商覆盖文件格式错误 {}", path.display()))
}

/// 覆盖文件形态：`{"providers": [ {name, display, base_url, docs_url, default_model, key_envs?} ]}`
/// 也允许裸数组。同名 name 整体替换内置条目，新名追加。
fn parse_overrides(raw: &str) -> Result<Vec<Provider>> {
    let v: Value = serde_json::from_str(raw)?;
    let arr = if let Some(a) = v.as_array() {
        a.clone()
    } else if let Some(a) = v.get("providers").and_then(|p| p.as_array()) {
        a.clone()
    } else {
        bail!("需要顶层数组或 {{\"providers\": [...]}}");
    };
    serde_json::from_value::<Vec<Provider>>(Value::Array(arr)).context(
        "providers 条目不符合结构（name/display/base_url/docs_url/default_model[/key_envs]）",
    )
}

fn merge(mut base: Vec<Provider>, overrides: Vec<Provider>) -> Vec<Provider> {
    for o in overrides {
        if let Some(existing) = base.iter_mut().find(|b| b.name == o.name) {
            *existing = o;
        } else {
            base.push(o);
        }
    }
    base.sort_by(|a, b| a.name.cmp(&b.name));
    base
}

/// 生效目录 = 内置 + 用户覆盖。
pub fn catalog() -> Result<Vec<Provider>> {
    Ok(merge(builtin(), load_overrides_file(&overrides_path()?)?))
}

pub fn find(name: &str) -> Result<Provider> {
    catalog()?
        .into_iter()
        .find(|p| p.name == name)
        .with_context(|| format!("未知供应商 {name:?}，可用 `icewright model providers` 查看列表"))
}

/// GET {base}/models 的响应解析：兼容 {"data":[{id}]} 与裸 [{id}] 两种形态。
fn extract_model_ids(v: &Value) -> Vec<String> {
    let arr = if let Some(a) = v.get("data").and_then(|d| d.as_array()) {
        a.clone()
    } else {
        v.as_array().cloned().unwrap_or_default()
    };
    let mut ids: Vec<String> = arr
        .iter()
        .filter_map(|m| m.get("id").and_then(Value::as_str).map(String::from))
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// 实时发现：调用 OpenAI-compatible 的 /models 列出现行可用模型名。
/// 这是"接入点/模型名会变动"的正面解法——配置时探活，不依赖任何写死清单。
pub fn list_models(base_url: &str, api_key: &str, timeout: Duration) -> Result<Vec<String>> {
    let url = format!("{}/models", base_url.trim().trim_end_matches('/'));
    let agent = ureq::AgentBuilder::new().timeout(timeout).build();
    let resp = agent
        .get(&url)
        .set("Authorization", &format!("Bearer {api_key}"))
        .call()
        .with_context(|| format!("模型列表请求失败 {url}（端点不可达或密钥无效）"))?;
    let value: Value = resp.into_json().context("端点返回的不是合法 JSON")?;
    let ids = extract_model_ids(&value);
    if ids.is_empty() {
        bail!("端点可达但未解析出任何模型 id，请检查返回结构或手动填写模型名");
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_shape() {
        let c = builtin();
        assert!(c.iter().any(|p| p.name == "deepseek"));
        for p in &c {
            assert!(p.base_url.starts_with("http"));
            assert!(!p.default_model.is_empty());
            assert!(!p.docs_url.is_empty());
        }
    }

    #[test]
    fn overrides_replace_and_extend() {
        let raw = r#"[{"name":"deepseek","display":"私有网关","base_url":"https://llm.corp.internal/v1","docs_url":"https://wiki.corp/llm","default_model":"ds-pro","key_envs":["CORP_KEY"]},{"name":"mylocal","display":"自部署 vLLM","base_url":"http://10.0.0.9:8000/v1","docs_url":"https://docs.vllm.ai","default_model":"qwen"}]"#;
        let merged = merge(builtin(), parse_overrides(raw).unwrap());
        let ds = merged.iter().find(|p| p.name == "deepseek").unwrap();
        assert_eq!(ds.base_url, "https://llm.corp.internal/v1");
        assert!(merged.iter().any(|p| p.name == "mylocal"));
        // 非法结构必须报错而不是静默
        assert!(parse_overrides(r#"{"foo":1}"#).is_err());
        assert!(parse_overrides(r#"[{"name":"x"}]"#).is_err());
    }

    #[test]
    fn extract_model_ids_both_shapes() {
        let wrapped: Value =
            serde_json::from_str(r#"{"data":[{"id":"b"},{"id":"a"},{"name":"nope"}]}"#).unwrap();
        assert_eq!(extract_model_ids(&wrapped), vec!["a", "b"]);
        let bare: Value = serde_json::from_str(r#"[{"id":"c"}]"#).unwrap();
        assert_eq!(extract_model_ids(&bare), vec!["c"]);
    }

    #[test]
    fn list_models_against_fake_endpoint() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            let body = r#"{"data":[{"id":"m2"},{"id":"m1"}]}"#;
            let _ = s.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            );
        });
        let ids = list_models(
            &format!("http://{addr}/v1"),
            "sk-fake",
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(ids, vec!["m1", "m2"]);
    }

    // 真实端点冒烟（需已配置密钥环境变量），CI 默认跳过。
    #[test]
    fn deepseek_models_live_if_configured() {
        let Some(key) = std::env::var("DEEPSEEK_API_KEY")
            .ok()
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .filter(|v| !v.is_empty())
        else {
            eprintln!("跳过：未设置 DEEPSEEK_API_KEY/OPENAI_API_KEY");
            return;
        };
        let p = find("deepseek").unwrap();
        let ids = list_models(&p.base_url, &key, Duration::from_secs(15)).unwrap();
        assert!(ids.iter().any(|m| m.contains("deepseek")), "got {ids:?}");
    }
}
