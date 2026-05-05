//! OpenAI Chat Completions provider with function calling forcing
//! structured output. The function schema mirrors the Anthropic tool
//! schema (both use JSON Schema), and we set `tool_choice` to force
//! invocation. The model's `tool_calls[0].function.arguments` is a
//! JSON string we deserialise into [`super::scene_gen::DraftScene`].
//!
//! Endpoint: `POST https://api.openai.com/v1/chat/completions`
//! Auth: `Authorization: Bearer <key>`.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::ai::scene_gen::{parse_draft_lenient, ContextFixture, DraftScene, SYSTEM_PROMPT};

const ENDPOINT: &str = "https://api.openai.com/v1/chat/completions";
const DEFAULT_MODEL: &str = "gpt-5";

pub async fn generate_scene(
    api_key: &str,
    model: &str,
    user_prompt: &str,
    step_count: u32,
    fixtures: &[ContextFixture],
    seed: Option<&DraftScene>,
) -> Result<DraftScene, String> {
    let model = if model.is_empty() { DEFAULT_MODEL } else { model };
    let body = build_request_body(model, user_prompt, step_count, fixtures, seed);

    let client = reqwest::Client::new();
    let resp = client
        .post(ENDPOINT)
        .bearer_auth(api_key)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("openai request failed: {e}"))?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("openai body read failed: {e}"))?;
    if !status.is_success() {
        return Err(format!("openai {status}: {text}"));
    }

    parse_function_call(&text)
}

/// Cheap probe: ask the model to echo "ok" with a 1-token cap.
pub async fn test_connection(api_key: &str, model: &str) -> Result<String, String> {
    let model = if model.is_empty() { DEFAULT_MODEL } else { model };
    let body = json!({
        "model": model,
        "max_tokens": 1,
        "messages": [{"role": "user", "content": "ping"}],
    });
    let client = reqwest::Client::new();
    let resp = client
        .post(ENDPOINT)
        .bearer_auth(api_key)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("openai request failed: {e}"))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("openai body read failed: {e}"))?;
    if !status.is_success() {
        return Err(format!("openai {status}: {text}"));
    }
    Ok(format!("OK · modelo {model} responde"))
}

fn build_request_body(
    model: &str,
    user_prompt: &str,
    step_count: u32,
    fixtures: &[ContextFixture],
    seed: Option<&DraftScene>,
) -> Value {
    let user_message = crate::ai::anthropic::build_user_message(
        user_prompt,
        step_count,
        fixtures,
        seed,
    );

    json!({
        "model": model,
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": user_message},
        ],
        "tools": [scene_function_schema()],
        "tool_choice": {"type": "function", "function": {"name": "create_scene"}},
    })
}

/// OpenAI nests the schema differently than Anthropic, but the
/// underlying parameters object is identical JSON Schema. We delegate
/// to the Anthropic helper so the two providers can't drift apart.
fn scene_function_schema() -> Value {
    let inner = crate::ai::anthropic::scene_tool_schema();
    json!({
        "type": "function",
        "function": {
            "name": "create_scene",
            "description": inner["description"].clone(),
            "parameters": inner["input_schema"].clone(),
        }
    })
}

#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
    /// "stop", "length", "tool_calls", "content_filter". `"length"`
    /// means the response was truncated; surface that distinctly.
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    tool_calls: Vec<ToolCall>,
}

#[derive(Deserialize)]
struct ToolCall {
    function: FunctionCall,
}

#[derive(Deserialize)]
struct FunctionCall {
    name: String,
    /// JSON string per OpenAI's contract — has to be parsed twice.
    arguments: String,
}

fn parse_function_call(body: &str) -> Result<DraftScene, String> {
    let resp: OpenAiResponse =
        serde_json::from_str(body).map_err(|e| format!("openai JSON parse: {e} · body: {body}"))?;
    let Some(choice) = resp.choices.into_iter().next() else {
        return Err("openai response sin choices".into());
    };
    let truncated = choice.finish_reason.as_deref() == Some("length");
    let Some(call) = choice
        .message
        .tool_calls
        .into_iter()
        .find(|c| c.function.name == "create_scene")
    else {
        return Err(if truncated {
            "OpenAI cortó la respuesta por length y no llegó a emitir el tool_call. \
             Probá con menos steps o menos fixtures."
                .into()
        } else {
            "openai no llamó a la function create_scene".into()
        });
    };
    // OpenAI hands us a JSON-as-string. Parse to Value first so the
    // lenient handler can recover from the same string-wrapping or
    // bare-array deviations we see on the Anthropic side.
    let value: Value = serde_json::from_str(&call.function.arguments).map_err(|e| {
        format!(
            "openai arguments no son JSON válido: {e} · args: {}",
            call.function.arguments
        )
    })?;
    parse_draft_lenient(value).map_err(|e| {
        if truncated {
            format!(
                "OpenAI cortó la respuesta por length — el draft quedó incompleto. \
                 Probá con menos fixtures o menos steps. ({e})"
            )
        } else {
            e
        }
    })
}
