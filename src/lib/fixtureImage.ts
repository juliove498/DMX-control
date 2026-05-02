import { convertFileSrc } from "@tauri-apps/api/core";

/// Resolve a fixture definition's `image` field into something the
/// browser can render via `<img src>`. The current backend stores images
/// inline as `data:image/...;base64,...` URLs so the JSON is portable;
/// older library dirs may still have the legacy form `images/foo.png`
/// — we keep handling that path too so existing installs keep working
/// until the user re-uploads.
export function fixtureImageSrc(
  image: string | null | undefined,
  libraryDir: string | null,
): string | null {
  if (!image) return null;
  if (image.startsWith("data:")) return image;
  if (!libraryDir) return null;
  return convertFileSrc(`${libraryDir}/${image}`);
}
