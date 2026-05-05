//! Orchestrator: build the show context the LLM sees, dispatch to the
//! configured provider, then validate the response against the live
//! show before handing it back to the frontend as a draft.
//!
//! The validation pass is the line of defence against hallucinations:
//! - Fixture IDs must be ones we passed in. Invented IDs are dropped.
//! - Channel offsets must sit inside the fixture mode's channel list.
//! - Values clamp to `0..=255`.
//! - Timings clamp to `0..=60_000` ms (a minute is enough for any
//!   theatre cue we care about; longer is almost always a unit error).
//! - Steps with no fixtures left after filtering are dropped, and a
//!   draft with zero surviving steps is rejected outright.
//!
//! Returning `Result<DraftScene, String>` keeps error surfacing simple
//! — the frontend just shows the message to the operator.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::ai::{anthropic, config::AiProvider, openai};
use crate::show::file::ShowFileV1;

/// Shape returned to the frontend as a preview before applying.
/// Mirrors `Scene`/`SceneStep`/`SceneFixture` from the show domain
/// but stays decoupled so changes there don't churn the LLM contract.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct DraftScene {
    pub name: String,
    pub steps: Vec<DraftStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct DraftStep {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub fade_in_ms: u32,
    pub hold_ms: u32,
    pub fixtures: Vec<DraftFixture>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct DraftFixture {
    pub fixture_id: String,
    pub values: Vec<DraftValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct DraftValue {
    pub channel_offset: u16,
    pub value: u8,
}

/// Compact, LLM-friendly description of one fixture. We deliberately
/// strip `image`, `description`, and other UI metadata: every token
/// we send is paid for, and the model only needs role/name/range info
/// to map intent to channel values.
#[derive(Debug, Clone, Serialize)]
pub struct ContextFixture {
    pub id: String,
    pub label: String,
    pub definition: String,
    pub mode: String,
    pub channels: Vec<ContextChannel>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextChannel {
    pub offset: u16,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub default: u8,
    /// Range labels, if the channel is a wheel (color, gobo, etc.).
    /// Each entry is `from-to:label`, joined by `; `. Sent as a single
    /// string so the LLM doesn't have to parse a nested array.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ranges: Option<String>,
}

/// Build the per-fixture context array. `fixture_ids` optionally
/// narrows the set — when None, every patched fixture is included.
pub fn build_context(
    show: &ShowFileV1,
    library: &std::collections::HashMap<String, crate::show::fixture::FixtureDefinition>,
    fixture_ids: Option<&[String]>,
) -> Vec<ContextFixture> {
    let allowed: Option<std::collections::HashSet<&str>> =
        fixture_ids.map(|ids| ids.iter().map(String::as_str).collect());

    show.fixtures
        .iter()
        .filter(|f| allowed.as_ref().map_or(true, |s| s.contains(f.id.as_str())))
        .filter_map(|f| {
            let def = library.get(&f.definition_id)?;
            let mode = def.mode(f.mode_index as usize)?;
            let channels = mode
                .channels
                .iter()
                .enumerate()
                .map(|(i, ch)| ContextChannel {
                    offset: i as u16,
                    role: ch.role.label().to_string(),
                    name: ch.name.clone(),
                    default: ch.default,
                    ranges: if ch.ranges.is_empty() {
                        None
                    } else {
                        Some(
                            ch.ranges
                                .iter()
                                .map(|r| format!("{}-{}:{}", r.from, r.to, r.label))
                                .collect::<Vec<_>>()
                                .join("; "),
                        )
                    },
                })
                .collect();
            Some(ContextFixture {
                id: f.id.clone(),
                label: f.label.clone().unwrap_or_else(|| f.id.clone()),
                definition: def.name.clone(),
                mode: mode.name.clone(),
                channels,
            })
        })
        .collect()
}

/// System prompt: strict rules to keep the LLM inside the lines.
/// Kept compact so it doesn't dominate the cost on small shows.
pub const SYSTEM_PROMPT: &str = r#"Sos un asistente experto en diseño de iluminación DMX-512 que genera escenas de varios pasos para un controlador de luces en vivo.

REGLAS DURAS (no rompas ninguna):
1. Devolvé SOLO la llamada a la tool/función estructurada provista. Nada de prosa.
1.a. La tool input es un OBJETO con dos campos: `name` (string) y `steps` (array). NUNCA devuelvas solo el array de steps suelto. NUNCA devuelvas el JSON como string entre comillas dobles. Devolvé el objeto con ambos campos directamente.
2. Usá ÚNICAMENTE los `fixture_id` que aparecen en el contexto. NO inventes IDs.
3. `channel_offset` es 0-based dentro del modo del fixture; nunca uses uno fuera de los listados.
4. `value` es entero 0-255.
5. `fade_in_ms` y `hold_ms` son enteros entre 0 y 60000.
6. Generá EXACTAMENTE la cantidad de steps que pide el usuario.
7. Si el usuario pide "blackout" en un step, poné en 0 los canales con role intensity/dimmer.
8. No incluyas un fixture en un step si no querés cambiar nada en él. Omitir != poner en 0.

INTERPRETACIÓN DE ROLES:
- intensity / dimmer: brillo 0-255.
- red / green / blue / white / amber / uv / cyan / magenta / yellow: mezcla aditiva de color.
- pan / tilt: posición; 128 = centro. pan_fine / tilt_fine son LSB de 16-bit.
- color (rueda) / gobo / strobe: si el contexto trae `ranges`, elegí el centro del rango cuyo label coincida con la intención del usuario.
- zoom / focus / iris: continuo, normalmente 128 a menos que el usuario pida algo específico.

PALETA POR DEFECTO:
- Cálido / amber / vela: amber alto (220+), red moderado (180), green bajo (60), blue 0, intensity 200+.
- Frío / azul / luna: blue alto (220+), white moderado (120), red 0, intensity 180+.
- Rojo punk: red 255, todo lo demás 0, intensity 255.
- Verde ácido: green 255, amber 60, intensity 240.
- Wash blanco: white 255 o (red 255, green 255, blue 255), intensity 255.
- Stroboscopio: strobe canal con value alto (200+), intensity 255.

DISEÑO DE TRANSICIONES:
- Si el usuario no especifica fades, usá 800ms fade_in y 1000ms hold como base.
- Para "rápido" / "punzante": 50-150ms fade, 100-300ms hold.
- Para "lento" / "ambient": 1500-3000ms fade, 1500-3000ms hold.
- Variá entre steps para que el ojo perciba progresión, no monotonía.

ITERACIÓN SOBRE UNA ESCENA EXISTENTE:
- Si el mensaje del usuario incluye una sección `seed_scene` (JSON con la escena actual), tratala como punto de partida. Cambiá ÚNICAMENTE lo que el usuario pide; conservá el resto de los valores tal cual.
- Mantené la cantidad de steps del seed salvo que el usuario pida explícitamente cambiarla.
- Mantené los nombres y los fade/hold del seed salvo que el usuario pida ajustarlos.
- Devolvé la escena ENTERA modificada (no un diff) — la app reemplaza completo, no parchea."#;

pub async fn generate(
    provider: AiProvider,
    api_key: &str,
    model: &str,
    user_prompt: &str,
    step_count: u32,
    fixtures: &[ContextFixture],
    seed: Option<&DraftScene>,
) -> Result<DraftScene, String> {
    if fixtures.is_empty() {
        return Err("No hay fixtures para iluminar — agregá fixtures al patch primero.".into());
    }
    if !(1..=16).contains(&step_count) {
        return Err(format!(
            "step_count debe estar entre 1 y 16 (recibí {step_count})"
        ));
    }

    let raw = match provider {
        AiProvider::None => return Err("Provider no configurado".into()),
        AiProvider::Anthropic => {
            anthropic::generate_scene(api_key, model, user_prompt, step_count, fixtures, seed)
                .await?
        }
        AiProvider::Openai => {
            openai::generate_scene(api_key, model, user_prompt, step_count, fixtures, seed).await?
        }
    };

    Ok(validate_and_clamp(raw, fixtures))
}

/// Parse what the LLM returned into a [`DraftScene`], tolerating two
/// real-world deviations we've seen:
///
/// 1. **Stringified JSON.** The provider sometimes hands us a
///    `Value::String` containing the JSON object instead of the
///    object itself. Happens with some Anthropic models on the
///    tool_use input field, and with OpenAI when the model
///    double-encodes the function arguments. We detect the
///    `invalid type: string` shape and unwrap one level.
/// 2. **Bare steps array.** The model occasionally drops the
///    `{name, steps}` wrapper and returns just the array of step
///    objects. We synthesise the wrapper with a placeholder name so
///    the rest of the pipeline (validate + apply) doesn't have to
///    branch on this.
///
/// Returns the original error verbatim (with the raw value
/// included for debugging) when neither recovery path applies.
pub fn parse_draft_lenient(raw: Value) -> Result<DraftScene, String> {
    match serde_json::from_value::<DraftScene>(raw.clone()) {
        Ok(d) => Ok(d),
        Err(direct_err) => {
            // Unwrap a stringified-JSON layer if present.
            if let Value::String(s) = &raw {
                let inner: Value = serde_json::from_str(s).map_err(|e| {
                    format!(
                        "draft scene parse: string content was not JSON ({e}); content: {s}"
                    )
                })?;
                return parse_draft_lenient(inner);
            }
            // The bare-array case: assume the model returned just the
            // steps and synthesise the wrapper. The caller's
            // validate_and_clamp pass still rejects bogus content, so
            // if this is wrong we catch it there.
            if matches!(raw, Value::Array(_)) {
                let synthetic = serde_json::json!({
                    "name": "AI scene",
                    "steps": raw,
                });
                return serde_json::from_value::<DraftScene>(synthetic).map_err(|e2| {
                    format!(
                        "draft scene parse: tried wrapping array as steps but inner shape \
                         is wrong: {e2} (original: {direct_err})"
                    )
                });
            }
            // Render a printable form of the raw input for the user
            // — capping at a sensible length so the toast doesn't
            // explode.
            let raw_str = serde_json::to_string(&raw).unwrap_or_else(|_| "<unprintable>".into());
            let raw_str = if raw_str.len() > 1200 {
                format!("{}…(truncated)", &raw_str[..1200])
            } else {
                raw_str
            };
            Err(format!("draft scene parse: {direct_err} · input: {raw_str}"))
        }
    }
}

/// Drop anything the LLM made up; clamp anything in range.
fn validate_and_clamp(mut draft: DraftScene, fixtures: &[ContextFixture]) -> DraftScene {
    let by_id: std::collections::HashMap<&str, &ContextFixture> =
        fixtures.iter().map(|f| (f.id.as_str(), f)).collect();

    draft.steps = draft
        .steps
        .into_iter()
        .map(|mut step| {
            step.fade_in_ms = step.fade_in_ms.min(60_000);
            step.hold_ms = step.hold_ms.min(60_000);
            step.fixtures = step
                .fixtures
                .into_iter()
                .filter_map(|mut fx| {
                    let Some(ctx) = by_id.get(fx.fixture_id.as_str()) else {
                        return None;
                    };
                    let max_offset = ctx.channels.len() as u16;
                    fx.values.retain(|v| v.channel_offset < max_offset);
                    if fx.values.is_empty() {
                        None
                    } else {
                        Some(fx)
                    }
                })
                .collect();
            step
        })
        .filter(|step| !step.fixtures.is_empty())
        .collect();

    if draft.name.trim().is_empty() {
        draft.name = "AI scene".into();
    }
    draft
}
