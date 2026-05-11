// Astro content collections. Starlight ships its own schema for the
// `docs` collection — registering it here is what wires our markdown
// files in src/content/docs/ into the site without manually defining
// frontmatter shape.

import { defineCollection } from "astro:content";
import { docsLoader } from "@astrojs/starlight/loaders";
import { docsSchema } from "@astrojs/starlight/schema";

export const collections = {
  docs: defineCollection({ loader: docsLoader(), schema: docsSchema() }),
};
