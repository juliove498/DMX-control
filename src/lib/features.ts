import type { Features } from "@bindings/Features";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

// Per-machine feature flags (instance.json, never synced). Off = the UI
// never mounts the module, so it costs nothing until switched on.

const DEFAULTS: Features = { cue_editor: false, preview3d: true, preview3d_quality: "medium" };

export function useFeatures(): Features {
  const [features, setFeatures] = useState<Features>(DEFAULTS);
  useEffect(() => {
    let alive = true;
    invoke<Features>("instance_features")
      .then((f) => {
        if (alive && f) setFeatures(f);
      })
      .catch(() => {
        // Doc mode / stubbed IPC: defaults.
      });
    const un = listen<Features>("features:changed", (e) => {
      if (alive && e.payload) setFeatures(e.payload);
    });
    return () => {
      alive = false;
      un.then((f) => f());
    };
  }, []);
  return features;
}

export async function saveFeatures(features: Features): Promise<Features> {
  return await invoke<Features>("set_instance_features", { features });
}
