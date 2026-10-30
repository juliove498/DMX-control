// Compatibility helpers for older WebKit (the 2015 Intel MacBook tops out at
// whatever Safari its macOS still gets). `crypto.randomUUID` and
// `structuredClone` only exist from Safari 15.4, and a missing API kills the
// click handler that called it — silently. Anything in our own code that needs
// them goes through these helpers instead.

export function newId(): string {
  const c: Crypto | undefined = globalThis.crypto;
  if (c && typeof c.randomUUID === "function") return c.randomUUID();
  const bytes = new Uint8Array(16);
  if (c && typeof c.getRandomValues === "function") {
    c.getRandomValues(bytes);
  } else {
    for (let i = 0; i < bytes.length; i++) bytes[i] = Math.floor(Math.random() * 256);
  }
  // RFC 4122 v4 markers so the result is shaped like what randomUUID gives.
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

/**
 * JSON round-trip clone. Only safe for plain data (our show/config state is
 * all serde-JSON on the other side anyway), which is exactly where we use it.
 */
export function deepClone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

/** Patch the missing globals so third-party code paths survive too. */
export function installCompatShims(): void {
  const g = globalThis as { structuredClone?: <T>(v: T) => T };
  if (typeof g.structuredClone !== "function") {
    g.structuredClone = deepClone;
  }
  try {
    const c = globalThis.crypto as (Crypto & { randomUUID?: Crypto["randomUUID"] }) | undefined;
    if (c && typeof c.randomUUID !== "function") {
      c.randomUUID = (() => newId()) as Crypto["randomUUID"];
    }
  } catch {
    // Some engines expose crypto as read-only; the newId() helper still
    // covers all first-party call sites.
  }
}
