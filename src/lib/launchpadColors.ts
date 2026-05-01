// Hex equivalents of the Launchpad MK2 palette indices used by the
// LaunchpadController in src-tauri/src/midi/launchpad.rs. Kept in sync
// manually — if you change PAD_PALETTE on the Rust side, update these
// to match so the UI badge colour reflects what the operator sees on
// the controller.
export const CHASER_PAD_COLORS = [
  "#ff1a00", // red       (palette 5)
  "#ff7700", // orange    (palette 9)
  "#ffe900", // yellow    (palette 13)
  "#1aff1a", // green     (palette 17)
  "#00e6e6", // cyan      (palette 37)
  "#33b5ff", // light blue (palette 41)
  "#3355ff", // blue      (palette 45)
  "#ff33cc", // magenta   (palette 53)
] as const;

export const MOVEMENT_PAD_COLORS = [
  "#ff80aa", // pink      (palette 95)
  "#aa55ff", // purple    (palette 81)
  "#ffaa00", // amber     (palette 84)
  "#88ff00", // lime      (palette 73)
  "#00ffaa", // teal      (palette 33)
  "#dd33dd", // fuchsia   (palette 49)
  "#ffaaff", // pale pink (palette 56)
  "#aabbff", // periwinkle (palette 99)
] as const;

export const BLACKOUT_COLOR = "#ff1a00"; // red, like the LP scene button
export const BLIND_COLOR = "#ffd966"; // amber/halogen, like the LP scene button

export function chaserColor(slotIndex: number): string {
  return CHASER_PAD_COLORS[slotIndex % CHASER_PAD_COLORS.length];
}

export function movementColor(slotIndex: number): string {
  return MOVEMENT_PAD_COLORS[slotIndex % MOVEMENT_PAD_COLORS.length];
}
