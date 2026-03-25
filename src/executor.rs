use anyhow::{Result, bail};
use serde_json::{Value, json};

use crate::config::AgentMeshConfig;
use crate::models::{DelegateRequestBody, LocalProfile};

pub async fn execute_delegate(
    config: &AgentMeshConfig,
    profile: &LocalProfile,
    request: &DelegateRequestBody,
) -> Result<Value> {
    match config.executor_mode.as_str() {
        "disabled" => bail!("local delegate executor is disabled"),
        "builtin" => Ok(execute_builtin(profile, request)),
        "openai_compat" => execute_openai_compat(config, profile, request).await,
        other => bail!("unsupported executor_mode: {other}"),
    }
}

pub fn delegate_available(config: &AgentMeshConfig) -> bool {
    config.executor_mode != "disabled"
}

fn execute_builtin(profile: &LocalProfile, request: &DelegateRequestBody) -> Value {
    let input_preview = preview_json(&request.input, request.max_output_chars.unwrap_or(320));
    let context_preview = request
        .context
        .as_ref()
        .map(|value| preview_json(value, 320))
        .unwrap_or_else(|| "none".to_string());
    let summary = format!(
        "builtin worker {} handled {} :: {} :: input={} :: context={}",
        profile
            .agent_label
            .clone()
            .unwrap_or_else(|| profile.peer_id.clone()),
        request.task_type,
        request.instruction.trim(),
        input_preview,
        context_preview
    );
    json!({
        "mode": "builtin",
        "node": profile.agent_label,
        "task_type": request.task_type,
        "instruction": request.instruction,
        "input_preview": input_preview,
        "context_preview": context_preview,
        "summary": truncate(summary, request.max_output_chars.unwrap_or(480)),
    })
}

async fn execute_openai_compat(
    config: &AgentMeshConfig,
    profile: &LocalProfile,
    request: &DelegateRequestBody,
) -> Result<Value> {
    let url = config
        .executor_url
        .clone()
        .ok_or_else(|| anyhow::anyhow!("executor_url is required for openai_compat mode"))?;
    let model = config
        .executor_model
        .clone()
        .unwrap_or_else(|| "default".to_string());
    let client = reqwest::Client::builder().build()?;
    let mut req = client.post(format!("{}/v1/chat/completions", url.trim_end_matches('/')));
    if let Some(env_name) = &config.executor_api_key_env {
        if let Ok(token) = std::env::var(env_name) {
            if !token.trim().is_empty() {
                req = req.bearer_auth(token);
            }
        }
    }
    let system = format!(
        "You are WildMesh's delegated worker for node {}. Return compact JSON only with keys: summary, output. Keep it concise and operational.",
        profile
            .agent_label
            .clone()
            .unwrap_or_else(|| profile.peer_id.clone())
    );
    let user = serde_json::to_string_pretty(&json!({
        "task_id": request.task_id,
        "task_type": request.task_type,
        "instruction": request.instruction,
        "input": request.input,
        "context": request.context,
        "max_output_chars": request.max_output_chars,
    }))?;
    let response = req
        .json(&json!({
            "model": model,
            "temperature": 0.1,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user}
            ]
        }))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    let content = response
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if let Ok(parsed) = serde_json::from_str::<Value>(&content) {
        return Ok(json!({
            "mode": "openai_compat",
            "model": config.executor_model,
            "result": parsed,
        }));
    }
    Ok(json!({
        "mode": "openai_compat",
        "model": config.executor_model,
        "summary": truncate(content.clone(), request.max_output_chars.unwrap_or(480)),
        "output": content,
    }))
}

fn preview_json(value: &Value, max_chars: usize) -> String {
    truncate(
        serde_json::to_string(value).unwrap_or_else(|_| "<unserializable>".to_string()),
        max_chars,
    )
}

fn truncate(value: String, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value;
    }
    value
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>()
        + "..."
}
