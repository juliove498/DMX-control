// Lightweight i18n: a React context that exposes the current language,
// a setter, and a `t()` function with `{name}` interpolation. No
// react-i18next — the app's translation surface is small and our
// dictionaries enforce keys at compile time via the `Translation` type
// in translations.ts, which gives strictly better type safety than the
// usual library setup.
//
// Storage: localStorage `dmx.language`. First-launch default tries the
// browser's `navigator.language`, falls back to Spanish (the operator's
// primary language).

import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";

import { DICTIONARIES, type LanguageCode, type Translation } from "./translations";

const STORAGE_KEY = "dmx.language";

type Substitutions = Record<string, string | number>;

type LanguageContext = {
  lang: LanguageCode;
  setLang: (next: LanguageCode) => void;
  t: <K extends keyof Translation>(key: K, vars?: Substitutions) => string;
};

const Ctx = createContext<LanguageContext | null>(null);

function detectInitial(): LanguageCode {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === "en" || stored === "es") return stored;
  } catch {
    // localStorage may throw in private modes / Tauri custom protocols;
    // fall through to detection.
  }
  const nav =
    typeof navigator !== "undefined" && typeof navigator.language === "string"
      ? navigator.language.toLowerCase()
      : "";
  if (nav.startsWith("en")) return "en";
  return "es";
}

function interpolate(template: string, vars?: Substitutions): string {
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (full, key) => {
    const v = vars[key];
    return v == null ? full : String(v);
  });
}

export function LanguageProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<LanguageCode>(() => detectInitial());

  const setLang = useCallback((next: LanguageCode) => {
    setLangState(next);
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // Same caveat as above — best-effort persistence; in-memory still works.
    }
  }, []);

  // Reflect the language on the <html lang="…"> attribute so screen
  // readers and the browser spellcheck pick up the right locale.
  useEffect(() => {
    if (typeof document !== "undefined") {
      document.documentElement.lang = lang;
    }
  }, [lang]);

  const value = useMemo<LanguageContext>(() => {
    const dict = DICTIONARIES[lang];
    return {
      lang,
      setLang,
      t: (key, vars) => interpolate(dict[key] ?? String(key), vars),
    };
  }, [lang, setLang]);

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useLanguage(): LanguageContext {
  const ctx = useContext(Ctx);
  if (!ctx) {
    throw new Error("useLanguage must be used inside <LanguageProvider>");
  }
  return ctx;
}

/// Convenience hook for components that only need `t`. Equivalent to
/// destructuring `useLanguage().t`, just shorter at the call site.
export function useT() {
  return useLanguage().t;
}

export type { LanguageCode } from "./translations";
export { LANGUAGES } from "./translations";
