import type { AiAvailableModels } from "@bindings/AiAvailableModels";
import type { AiConfig } from "@bindings/AiConfig";
import type { AiModelOption } from "@bindings/AiModelOption";
import type { AiProvider } from "@bindings/AiProvider";
import { useEffect, useState } from "react";
import { useShowStore } from "../stores/show";

/// IA settings panel. Lives under Config → IA. Persists to OS
/// app-config dir (off the show file — keys don't travel with shows).
export function AiConfigView() {
  const getAiConfig = useShowStore((s) => s.getAiConfig);
  const setAiConfigFn = useShowStore((s) => s.setAiConfig);
  const aiListModels = useShowStore((s) => s.aiListModels);
  const aiTestConnection = useShowStore((s) => s.aiTestConnection);

  const [config, setConfig] = useState<AiConfig | null>(null);
  const [models, setModels] = useState<AiAvailableModels | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showAnthKey, setShowAnthKey] = useState(false);
  const [showOpenAiKey, setShowOpenAiKey] = useState(false);
  const [testing, setTesting] = useState(false);

  useEffect(() => {
    let cancelled = false;
    Promise.all([getAiConfig(), aiListModels()])
      .then(([cfg, ms]) => {
        if (cancelled) return;
        setConfig(cfg);
        setModels(ms);
      })
      .catch((e) => setError(stringifyError(e)));
    return () => {
      cancelled = true;
    };
  }, [getAiConfig, aiListModels]);

  if (!config || !models) {
    return (
      <main className="page ai-config-view">
        <header className="page-head">
          <h2>IA</h2>
        </header>
        <p className="empty">Cargando…</p>
      </main>
    );
  }

  const update = (patch: Partial<AiConfig>) =>
    setConfig((prev) => (prev ? { ...prev, ...patch } : prev));
  const updateAnthropic = (patch: Partial<AiConfig["anthropic"]>) =>
    setConfig((prev) =>
      prev ? { ...prev, anthropic: { ...prev.anthropic, ...patch } } : prev,
    );
  const updateOpenAi = (patch: Partial<AiConfig["openai"]>) =>
    setConfig((prev) =>
      prev ? { ...prev, openai: { ...prev.openai, ...patch } } : prev,
    );

  const onSave = async () => {
    setError(null);
    setStatus(null);
    try {
      await setAiConfigFn(config);
      setStatus("Configuración guardada.");
    } catch (e) {
      setError(stringifyError(e));
    }
  };

  const onTest = async () => {
    setError(null);
    setStatus(null);
    setTesting(true);
    try {
      // Save first so the backend reads the just-edited values rather
      // than the stale-on-disk version.
      await setAiConfigFn(config);
      const msg = await aiTestConnection();
      setStatus(msg);
    } catch (e) {
      setError(stringifyError(e));
    } finally {
      setTesting(false);
    }
  };

  const activeProviderConfigured =
    (config.provider === "anthropic" && config.anthropic.api_key.length > 0) ||
    (config.provider === "openai" && config.openai.api_key.length > 0);

  return (
    <main className="page ai-config-view">
      <header className="page-head">
        <h2>IA</h2>
        <span className="meta">
          Generación de escenas con LLM (Anthropic / OpenAI) — POC
        </span>
      </header>

      <p className="hint ai-warning">
        Las API keys se guardan en texto plano en el directorio de configuración del SO
        (fuera del archivo del show). En producción deberían moverse al keychain del SO.
      </p>

      <section className="config-section">
        <h3>Provider activo</h3>
        <div className="ai-row">
          <label>
            <input
              type="radio"
              name="ai-provider"
              checked={config.provider === "none"}
              onChange={() => update({ provider: "none" as AiProvider })}
            />
            Desactivado
          </label>
          <label>
            <input
              type="radio"
              name="ai-provider"
              checked={config.provider === "anthropic"}
              onChange={() => update({ provider: "anthropic" as AiProvider })}
            />
            Anthropic (Claude)
          </label>
          <label>
            <input
              type="radio"
              name="ai-provider"
              checked={config.provider === "openai"}
              onChange={() => update({ provider: "openai" as AiProvider })}
            />
            OpenAI (GPT)
          </label>
        </div>
      </section>

      <section className="config-section">
        <h3>Anthropic</h3>
        <div className="ai-row">
          <label className="ai-key-label">
            API key
            <input
              type={showAnthKey ? "text" : "password"}
              value={config.anthropic.api_key}
              onChange={(e) => updateAnthropic({ api_key: e.currentTarget.value })}
              placeholder="sk-ant-…"
              autoComplete="off"
              spellCheck={false}
            />
          </label>
          <button
            type="button"
            className="ai-eye-btn"
            onClick={() => setShowAnthKey((v) => !v)}
            title={showAnthKey ? "Ocultar key" : "Mostrar key"}
          >
            {showAnthKey ? "🙈" : "👁"}
          </button>
        </div>
        <ModelPicker
          value={config.anthropic.model}
          options={models.anthropic}
          onChange={(model) => updateAnthropic({ model })}
        />
        <p className="hint">
          Conseguí una API key en{" "}
          <code>https://console.anthropic.com/settings/keys</code>.
        </p>
      </section>

      <section className="config-section">
        <h3>OpenAI</h3>
        <div className="ai-row">
          <label className="ai-key-label">
            API key
            <input
              type={showOpenAiKey ? "text" : "password"}
              value={config.openai.api_key}
              onChange={(e) => updateOpenAi({ api_key: e.currentTarget.value })}
              placeholder="sk-…"
              autoComplete="off"
              spellCheck={false}
            />
          </label>
          <button
            type="button"
            className="ai-eye-btn"
            onClick={() => setShowOpenAiKey((v) => !v)}
            title={showOpenAiKey ? "Ocultar key" : "Mostrar key"}
          >
            {showOpenAiKey ? "🙈" : "👁"}
          </button>
        </div>
        <ModelPicker
          value={config.openai.model}
          options={models.openai}
          onChange={(model) => updateOpenAi({ model })}
        />
        <p className="hint">
          Conseguí una API key en <code>https://platform.openai.com/api-keys</code>.
        </p>
      </section>

      <div className="ai-actions">
        <button type="button" onClick={onSave}>
          Guardar
        </button>
        <button
          type="button"
          onClick={onTest}
          disabled={!activeProviderConfigured || testing}
          title={
            !activeProviderConfigured
              ? "Elegí un provider y completá la API key primero"
              : "Hacer un request mínimo para verificar la API key + modelo"
          }
        >
          {testing ? "Probando…" : "Probar conexión"}
        </button>
      </div>

      {status ? (
        <output className="ai-status ok" aria-live="polite">
          {status}
        </output>
      ) : null}
      {error ? (
        <output className="ai-status err" aria-live="polite">
          Error: {error}
        </output>
      ) : null}
    </main>
  );
}

function ModelPicker({
  value,
  options,
  onChange,
}: {
  value: string;
  options: AiModelOption[];
  onChange: (id: string) => void;
}) {
  const effective = value || (options[0]?.id ?? "");
  const hint = options.find((o) => o.id === effective)?.hint ?? "";
  return (
    <div className="ai-row">
      <label className="ai-model-label">
        Modelo
        <select value={effective} onChange={(e) => onChange(e.currentTarget.value)}>
          {options.map((o) => (
            <option key={o.id} value={o.id}>
              {o.label}
            </option>
          ))}
        </select>
      </label>
      {hint ? <span className="ai-model-hint">{hint}</span> : null}
    </div>
  );
}

function stringifyError(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message?: unknown }).message ?? e);
  }
  return JSON.stringify(e);
}
