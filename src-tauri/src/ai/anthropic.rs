//! Anthropic Messages API provider with tool use forcing structured
//! output. We define a single tool — `create_scene` — whose schema
//! mirrors [`super::scene_gen::DraftScene`], then set `tool_choice` to
//! force the model to call it. The model's `input` for that tool call
//! deserialises directly into the draft.
//!
//! Endpoint: `POST https://api.anthropic.com/v1/messages`
//! Auth: `x-api-key` header (NOT bearer).
//! Versioning: `anthropic-version: 2023-06-01` is the stable API.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::ai::scene_gen::{parse_draft_lenient, ContextFixture, DraftScene, SYSTEM_PROMPT};

const ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";
const DEFAULT_MODEL: &str = "claude-sonnet-4-6";

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
        .header("x-api-key", api_key)
        .header("anthropic-version", API_VERSION)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("anthropic request failed: {e}"))?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("anthropic body read failed: {e}"))?;
    if !status.is_success() {
        return Err(format!("anthropic {status}: {text}"));
    }

    parse_tool_call(&text)
}

/// Lightweight ping that just verifies the key works and the chosen
/// model accepts requests. Calls `messages` with a 1-token cap so the
/// cost is essentially zero.
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
        .header("x-api-key", api_key)
        .header("anthropic-version", API_VERSION)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("anthropic request failed: {e}"))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("anthropic body read failed: {e}"))?;
    if !status.is_success() {
        return Err(format!("anthropic {status}: {text}"));
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
    let user_message = build_user_message(user_prompt, step_count, fixtures, seed);

    json!({
        "model": model,
        // 8192 covers ~12 steps × 30 fixtures × 8 channels of structured
        // tool output without truncation. When this gets hit, Anthropic
        // returns the partial `input` object as JSON (often just
        // `{"name": "..."}`) and parsing fails with "missing field
        // 'steps'", which is the exact symptom we hit at 4096.
        "max_tokens": 8192,
        "system": SYSTEM_PROMPT,
        "tools": [scene_tool_schema()],
        "tool_choice": {"type": "tool", "name": "create_scene"},
        "messages": [{"role": "user", "content": user_message}],
    })
}

/// Shared message-building helper. Both providers send the same body
/// shape — sections in order: step count, instruction, optional seed,
/// fixture context. Putting the seed BEFORE the fixture context keeps
/// the LLM's attention on "this is what to modify"; the fixture list
/// is reference material it dips into when it needs a channel offset.
pub(crate) fn build_user_message(
    user_prompt: &str,
    step_count: u32,
    fixtures: &[ContextFixture],
    seed: Option<&DraftScene>,
) -> String {
    let fixtures_json = serde_json::to_string(fixtures).unwrap_or_else(|_| "[]".into());
    match seed {
        Some(s) => {
            let seed_json = serde_json::to_string(s).unwrap_or_else(|_| "{}".into());
            format!(
                "Cantidad de steps requerida: {step_count}\n\
                 Instrucción del usuario sobre la escena actual: {user_prompt}\n\n\
                 seed_scene (escena actual a modificar, JSON):\n{seed_json}\n\n\
                 Contexto del show (fixtures disponibles, JSON):\n{fixtures_json}"
            )
        }
        None => format!(
            "Cantidad de steps requerida: {step_count}\n\
             Pedido del usuario: {user_prompt}\n\n\
             Contexto del show (fixtures disponibles, JSON):\n{fixtures_json}"
        ),
    }
}

/// JSON Schema for the `create_scene` tool. Anthropic uses standard
/// JSON Schema; OpenAI uses the same dialect for function-calling, so
/// the openai module borrows this same shape.
pub fn scene_tool_schema() -> Value {
    json!({
        "name": "create_scene",
        "description": "Devuelve la escena solicitada con la cantidad exacta de pasos pedida.",
        "input_schema": {
            "type": "object",
            "required": ["name", "steps"],
            "properties": {
                "name": {"type": "string", "description": "Nombre corto y descriptivo de la escena."},
                "steps": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 16,
                    "items": {
                        "type": "object",
                        "required": ["fade_in_ms", "hold_ms", "fixtures"],
                        "properties": {
                            "name": {"type": "string"},
                            "fade_in_ms": {"type": "integer", "minimum": 0, "maximum": 60000},
                            "hold_ms": {"type": "integer", "minimum": 0, "maximum": 60000},
                            "fixtures": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "required": ["fixture_id", "values"],
                                    "properties": {
                                        "fixture_id": {"type": "string"},
                                        "values": {
                                            "type": "array",
                                            "items": {
                                                "type": "object",
                                                "required": ["channel_offset", "value"],
                                                "properties": {
                                                    "channel_offset": {"type": "integer", "minimum": 0, "maximum": 511},
                                                    "value": {"type": "integer", "minimum": 0, "maximum": 255}
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<ContentBlock>,
    /// "end_turn", "tool_use", "max_tokens", "stop_sequence". When it
    /// equals "max_tokens" we know the response was cut off and the
    /// tool_use input is partial — surface that explicitly to the
    /// operator instead of the cryptic serde "missing field" error.
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentBlock {
    Text { text: String },
    ToolUse { name: String, input: Value },
}

fn parse_tool_call(body: &str) -> Result<DraftScene, String> {
    let resp: AnthropicResponse =
        serde_json::from_str(body).map_err(|e| format!("anthropic JSON parse: {e} · body: {body}"))?;
    let truncated = resp.stop_reason.as_deref() == Some("max_tokens");
    for block in resp.content {
        match block {
            ContentBlock::ToolUse { name, input } if name == "create_scene" => {
                return parse_draft_lenient(input).map_err(|e| {
                    if truncated {
                        format!(
                            "Anthropic cortó la respuesta por max_tokens — el draft quedó incompleto. \
                             Probá con menos fixtures o menos steps. ({e})"
                        )
                    } else {
                        e
                    }
                });
            }
            ContentBlock::Text { text } => {
                tracing::debug!(target: "ai", text = %text, "anthropic text block (ignored)");
            }
            _ => {}
        }
    }
    Err(if truncated {
        "Anthropic cortó la respuesta por max_tokens y no llegó a emitir el tool_use. \
         Probá con menos steps o menos fixtures."
            .into()
    } else {
        "anthropic no devolvió tool_use create_scene".into()
    })
}
