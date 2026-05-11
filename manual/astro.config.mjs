// Astro + Starlight config for the DMX Control manual.
//
// Single-locale (es) for now. Adding EN later means: introduce a
// `locales` map here and move existing docs under src/content/docs/es/
// — no other code change needed. Keeping it flat until then avoids
// pointless URL noise like /es/stage when there's only one language.

import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

export default defineConfig({
  site: "https://dmx-control.local",
  integrations: [
    starlight({
      title: "DMX Control",
      description: "Manual del operador — control DMX para shows en vivo.",
      defaultLocale: "es",
      locales: {
        root: { label: "Español", lang: "es" },
      },
      sidebar: [
        {
          label: "Primeros pasos",
          items: [{ label: "Bienvenida", slug: "index" }],
        },
        {
          label: "Operación en vivo",
          items: [
            { label: "El Stage", slug: "stage" },
            { label: "Escenas", slug: "escenas" },
          ],
        },
        {
          label: "Programación",
          items: [{ label: "Patch", slug: "patch" }],
        },
        {
          label: "Generadores ambientales",
          items: [
            { label: "Chasers", slug: "chasers" },
            { label: "Movements", slug: "movements" },
          ],
        },
        {
          label: "Control externo",
          items: [{ label: "Stream Deck", slug: "streamdeck" }],
        },
      ],
    }),
  ],
});
