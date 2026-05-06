// Single source of truth for every user-facing string in the desktop UI.
// Keep the EN and ES objects shape-identical: TypeScript pins both via
// `Translation` so a forgotten key in either locale fails the build
// instead of silently rendering an English string in a Spanish UI (or
// vice-versa). Add new keys to BOTH locales in the same commit.
//
// Migration policy: components opt in by replacing literal strings with
// `t("key")` one screen at a time. Strings not yet migrated stay
// hardcoded — that's how we ship i18n without a 100-file PR.

export type Translation = {
  // ---- App shell --------------------------------------------------------
  "app.brand": string;
  "app.tab.stage": string;
  "app.tab.scenes": string;
  "app.tab.chaser": string;
  "app.tab.movement": string;
  "app.tab.preview3d": string;
  "app.tab.config": string;
  "app.openInOtherWindow": string;
  "app.fullscreen": string;
  "app.fullscreenHint": string;
  "app.button.new": string;
  "app.button.open": string;
  "app.button.save": string;
  "app.global.blackout": string;
  "app.global.blackoutTitle": string;
  "app.global.blind": string;
  "app.global.blindTitle": string;
  "app.show.untitled": string;
  "app.show.unsaved": string;
  "app.show.renameHint": string;
  "app.show.renameHintEmpty": string;
  "app.dialog.closeTitle": string;
  "app.dialog.closeBody": string;
  "app.toast.savedAt": string;
  "app.toast.saveError": string;
  "app.toast.renameError": string;

  // ---- Config tabs ------------------------------------------------------
  "config.tabs.library": string;
  "config.tabs.libraryHint": string;
  "config.tabs.outputs": string;
  "config.tabs.outputsHint": string;
  "config.tabs.patch": string;
  "config.tabs.patchHint": string;
  "config.tabs.blackoutBlind": string;
  "config.tabs.blackoutBlindHint": string;
  "config.tabs.midi": string;
  "config.tabs.midiHint": string;
  "config.tabs.ai": string;
  "config.tabs.aiHint": string;
  "config.tabs.remote": string;
  "config.tabs.remoteHint": string;
  "config.tabs.sync": string;
  "config.tabs.syncHint": string;
  "config.tabs.general": string;
  "config.tabs.generalHint": string;
  "config.tabs.direct": string;
  "config.tabs.directHint": string;

  // ---- General (language picker) ----------------------------------------
  "general.title": string;
  "general.language": string;
  "general.languageHint": string;
  "general.language.en": string;
  "general.language.es": string;
  "general.showFile": string;
  "general.showFileHint": string;

  // ---- Sync view --------------------------------------------------------
  "sync.title": string;
  "sync.intro": string;
  "sync.section.token": string;
  "sync.token.saved": string;
  "sync.token.delete": string;
  "sync.token.deleteConfirm": string;
  "sync.token.placeholder": string;
  "sync.token.save": string;
  "sync.token.saving": string;
  "sync.token.connectedAs": string;
  "sync.token.help": string;
  "sync.section.settings": string;
  "sync.machineLabel": string;
  "sync.machineLabel.placeholder": string;
  "sync.gistId": string;
  "sync.gistId.placeholder": string;
  "sync.includeOutputs": string;
  "sync.saveSettings": string;
  "sync.section.status": string;
  "sync.status.lastPush": string;
  "sync.status.lastPull": string;
  "sync.status.lastRemote": string;
  "sync.status.never": string;
  "sync.status.from": string;
  "sync.probe": string;
  "sync.probing": string;
  "sync.probe.empty": string;
  "sync.probe.ahead": string;
  "sync.probe.upToDate": string;
  "sync.section.actions": string;
  "sync.action.push": string;
  "sync.action.pushing": string;
  "sync.action.forcePush": string;
  "sync.action.forcePushTitle": string;
  "sync.action.pull": string;
  "sync.action.pulling": string;
  "sync.action.pullConfirm": string;
  "sync.backupHint": string;
  "sync.toast.pushOk": string;
  "sync.toast.pullOk": string;

  // ---- Remote bridge view ----------------------------------------------
  "remote.title": string;
  "remote.intro": string;
  "remote.status": string;
  "remote.statusRunning": string;
  "remote.statusStopped": string;
  "remote.start": string;
  "remote.stop": string;
  "remote.connectManually": string;
  "remote.clientsConnected": string;
  "remote.pairing": string;
  "remote.pairing.expiresIn": string;
  "remote.pairing.cancel": string;
  "remote.pairing.start": string;
  "remote.devices": string;
  "remote.devices.empty": string;
  "remote.devices.pairedAgo": string;
  "remote.devices.seenAgo": string;
  "remote.devices.revoke": string;
  "remote.devices.revokeConfirm": string;
  "remote.time.secAgo": string;
  "remote.time.minAgo": string;
  "remote.time.hAgo": string;
  "remote.time.dAgo": string;

  // ---- Common buttons ---------------------------------------------------
  "common.cancel": string;
  "common.save": string;
  "common.delete": string;
  "common.loading": string;
  "common.empty": string;
  "common.add": string;
  "common.remove": string;
  "common.edit": string;
  "common.close": string;
  "common.refresh": string;
  "common.connect": string;
  "common.disconnect": string;
  "common.connected": string;
  "common.disconnected": string;
  "common.universe": string;
  "common.address": string;
  "common.channel": string;
  "common.fixture": string;
  "common.fixtures": string;
  "common.mode": string;
  "common.name": string;
  "common.value": string;
  "common.enabled": string;
  "common.disabled": string;
  "common.optional": string;
  "common.yes": string;
  "common.no": string;

  // ---- Direct output (debug faders) ------------------------------------
  "direct.title": string;
  "direct.fpsNone": string;
  "direct.fpsValue": string;
  "direct.statsMeta": string;
  "direct.master": string;
  "direct.blackoutOn": string;
  "direct.blackout": string;
  "direct.clearAll": string;

  // ---- Library (fixture defs) ------------------------------------------
  "library.title": string;
  "library.search": string;
  "library.reload": string;
  "library.uploadImage": string;
  "library.changeImage": string;
  "library.errorOpenDialog": string;
  "library.errorSetImage": string;
  "library.modeSummary": string;
  "library.filesPath": string;

  // ---- Outputs ---------------------------------------------------------
  "outputs.title": string;
  "outputs.kind.mock": string;
  "outputs.kind.artNet": string;
  "outputs.kind.sacn": string;
  "outputs.kind.enttec": string;
  "outputs.kind.openDmx": string;
  "outputs.kind.openDmxFtdi": string;
  "outputs.add.mock": string;
  "outputs.add.artNet": string;
  "outputs.add.sacn": string;
  "outputs.add.enttec": string;
  "outputs.add.openDmx": string;
  "outputs.add.openDmxFtdi": string;
  "outputs.rescan": string;
  "outputs.field.target": string;
  "outputs.field.sourceName": string;
  "outputs.field.priority": string;
  "outputs.field.serialPort": string;
  "outputs.field.ftdiDevice": string;
  "outputs.field.dtrHigh": string;
  "outputs.field.rtsHigh": string;
  "outputs.field.universes": string;
  "outputs.placeholder.select": string;
  "outputs.tag.ftdi": string;
  "outputs.tag.offline": string;
  "outputs.tag.inUse": string;
  "outputs.zadigHint": string;
  "outputs.remove": string;
  "outputs.empty": string;
  "outputs.dirty": string;
  "outputs.ok": string;
  "outputs.discard": string;
  "outputs.apply": string;

  // ---- Patch -----------------------------------------------------------
  "patch.title": string;
  "patch.statusClean": string;
  "patch.statusIssues": string;
  "patch.qty": string;
  "patch.universe": string;
  "patch.address": string;
  "patch.labelPlaceholder": string;
  "patch.add": string;
  "patch.modeFmt": string;
  "patch.col.id": string;
  "patch.col.label": string;
  "patch.col.fixture": string;
  "patch.col.mode": string;
  "patch.col.universe": string;
  "patch.col.address": string;
  "patch.col.range": string;
  "patch.del": string;
  "patch.unknownDef": string;
  "patch.conflictFmt": string;

  // ---- Buttons (blackout & blind) --------------------------------------
  "buttons.title": string;
  "buttons.subhead": string;
  "buttons.section.blackout": string;
  "buttons.section.blind": string;
  "buttons.blackoutIntro": string;
  "buttons.blindIntro": string;
  "buttons.fadeIn": string;
  "buttons.fadeOut": string;
  "buttons.assignedBlackout": string;
  "buttons.assignedBlackoutAuto": string;
  "buttons.assignedBlind": string;
  "buttons.emptyBlackout": string;
  "buttons.emptyBlind": string;
  "buttons.blackoutHintAuto": string;
  "buttons.blackoutHintCustom": string;
  "buttons.blindHintAuto": string;
  "buttons.blindHintCustom": string;

  // ---- MIDI ------------------------------------------------------------
  "midi.title": string;
  "midi.intro": string;
  "midi.devices": string;
  "midi.refresh": string;
  "midi.connect": string;
  "midi.disconnect": string;
  "midi.connected": string;
  "midi.notConnected": string;
  "midi.lastEvent": string;
  "midi.colorTest": string;
  "midi.empty": string;
  "midi.refreshHint": string;
  "midi.detectedDevices": string;
  "midi.errListing": string;
  "midi.errConnect": string;
  "midi.errDisconnect": string;
  "midi.errTest": string;
  "midi.in": string;
  "midi.out": string;
  "midi.bothIO": string;
  "midi.onlyIn": string;
  "midi.surfaceActive": string;
  "midi.testPads": string;
  "midi.statusSection": string;
  "midi.connectedToFmt": string;

  // ---- AI Config -------------------------------------------------------
  "ai.title": string;
  "ai.intro": string;
  "ai.provider": string;
  "ai.provider.anthropic": string;
  "ai.provider.openai": string;
  "ai.apiKey": string;
  "ai.apiKey.show": string;
  "ai.apiKey.hide": string;
  "ai.model": string;
  "ai.test": string;
  "ai.testing": string;
  "ai.testOk": string;
  "ai.testFail": string;
  "ai.save": string;
  "ai.saved": string;
  "ai.refreshModels": string;
  "ai.subhead": string;
  "ai.warning": string;
  "ai.providerActive": string;
  "ai.provider.none": string;
  "ai.provider.anthropicLong": string;
  "ai.provider.openaiLong": string;
  "ai.section.anthropic": string;
  "ai.section.openai": string;
  "ai.keyHint.anthropic": string;
  "ai.keyHint.openai": string;
  "ai.toggleKey.show": string;
  "ai.toggleKey.hide": string;
  "ai.testDisabledHint": string;
  "ai.testActiveHint": string;
  "ai.savedToast": string;
  "ai.errPrefix": string;
};

export const en: Translation = {
  "app.brand": "DMX Control",
  "app.tab.stage": "Stage",
  "app.tab.scenes": "Scenes",
  "app.tab.chaser": "Chaser",
  "app.tab.movement": "Movement",
  "app.tab.preview3d": "Preview 3D",
  "app.tab.config": "Config",
  "app.openInOtherWindow": "Open {name} in another window",
  "app.fullscreen": "Fullscreen",
  "app.fullscreenHint": "Fullscreen (F11)",
  "app.button.new": "New",
  "app.button.open": "Open…",
  "app.button.save": "Save",
  "app.global.blackout": "BLACKOUT",
  "app.global.blackoutTitle": "Blackout (toggle, configurable fades)",
  "app.global.blind": "BLIND",
  "app.global.blindTitle": "Blind / blinder (hold, halogen with fade in/out)",
  "app.show.untitled": "Untitled",
  "app.show.unsaved": "(unsaved) · click to rename",
  "app.show.renameHint": "{path} · click to rename",
  "app.show.renameHintEmpty": "(unsaved) · click to rename",
  "app.dialog.closeTitle": "Close app",
  "app.dialog.closeBody": "Close DMX Control?\nAll open popout windows will close too.",
  "app.toast.savedAt": "Saved to {path}",
  "app.toast.saveError": "Save failed: {err}",
  "app.toast.renameError": "Could not rename: {err}",

  "config.tabs.library": "Library",
  "config.tabs.libraryHint": "Fixture definitions",
  "config.tabs.outputs": "Outputs",
  "config.tabs.outputsHint": "DMX universes and drivers",
  "config.tabs.patch": "Patch",
  "config.tabs.patchHint": "Assign fixtures to DMX addresses",
  "config.tabs.blackoutBlind": "Blackout & Blind",
  "config.tabs.blackoutBlindHint": "Fades and fixtures affected by the global buttons",
  "config.tabs.midi": "MIDI",
  "config.tabs.midiHint": "Control surface (Launchpad, etc.)",
  "config.tabs.ai": "AI",
  "config.tabs.aiHint": "Provider, model and API key for LLM scene generation",
  "config.tabs.remote": "Remote",
  "config.tabs.remoteHint": "LAN bridge for the iPhone companion app",
  "config.tabs.sync": "Sync",
  "config.tabs.syncHint": "Share config across machines via private GitHub Gist",
  "config.tabs.general": "General",
  "config.tabs.generalHint": "Language and other preferences",
  "config.tabs.direct": "Direct",
  "config.tabs.directHint": "Raw per-channel faders — debug tool",

  "general.title": "General",
  "general.language": "Language",
  "general.languageHint": "Applies immediately. Saved per user in this device.",
  "general.language.en": "English",
  "general.language.es": "Español",
  "general.showFile": "Show file",
  "general.showFileHint":
    "Create a new show, open one from disk, or save the current one to a chosen path.",

  "sync.title": "Sync",
  "sync.intro":
    "Share config across your machines via a private GitHub Gist. Each user uses their own PAT and their own gist — nothing shared by default. Outputs are excluded from push by default (hardware usually differs between Mac and Windows).",
  "sync.section.token": "GitHub PAT",
  "sync.token.saved": "PAT saved in the keychain.",
  "sync.token.delete": "Delete",
  "sync.token.deleteConfirm": "Delete the PAT from the keychain?",
  "sync.token.placeholder": "ghp_… (scope: gist)",
  "sync.token.save": "Save",
  "sync.token.saving": "Saving…",
  "sync.token.connectedAs": "Connected as {user}.",
  "sync.token.help":
    "Generate a PAT at github.com/settings/tokens with the 'gist' scope. The token is stored in the OS keychain (macOS Keychain / Windows Credential Manager) — never in a file.",
  "sync.section.settings": "Settings",
  "sync.machineLabel": "Label for this machine",
  "sync.machineLabel.placeholder": "e.g. mac-julio, windows-studio",
  "sync.gistId": "Gist id",
  "sync.gistId.placeholder": "leave empty so the first Push creates one",
  "sync.includeOutputs": "Include Outputs in sync (only if both machines use the same hardware)",
  "sync.saveSettings": "Save settings",
  "sync.section.status": "Status",
  "sync.status.lastPush": "Last push:",
  "sync.status.lastPull": "Last pull:",
  "sync.status.lastRemote": "Last remote version seen:",
  "sync.status.never": "never",
  "sync.status.from": "from {machine}",
  "sync.probe": "Check remote",
  "sync.probing": "Checking…",
  "sync.probe.empty": "Remote: empty",
  "sync.probe.ahead": "Remote: {time} · {machine} — ahead, you should Pull first",
  "sync.probe.upToDate": "Remote: {time} · {machine} — up to date",
  "sync.section.actions": "Actions",
  "sync.action.push": "Push",
  "sync.action.pushing": "Pushing…",
  "sync.action.forcePush": "Force push",
  "sync.action.forcePushTitle": "Overwrite the remote without checking if it's ahead",
  "sync.action.pull": "Pull",
  "sync.action.pulling": "Pulling…",
  "sync.action.pullConfirm":
    "Pull will overwrite the current show with the gist version. Continue?",
  "sync.backupHint":
    "Before each Pull the app saves a backup of the current show under sync-backups/. The newest file is your pre-pull show.",
  "sync.toast.pushOk": "Push OK — gist {id}…, remote {time}",
  "sync.toast.pullOk": "Pull OK — from {machine}, remote {time}. Backup in sync-backups/.",

  "remote.title": "Remote (mobile)",
  "remote.intro":
    "LAN bridge for the iPhone companion app. Serves scenes / master / blackout / blind over local WiFi. No TLS — only on trusted networks. On Windows the firewall usually asks for permission the first time it starts: accept at least 'private networks'.",
  "remote.status": "Status:",
  "remote.statusRunning": "running",
  "remote.statusStopped": "stopped",
  "remote.start": "Start",
  "remote.stop": "Stop",
  "remote.connectManually": "Connect manually:",
  "remote.clientsConnected": "Clients connected:",
  "remote.pairing": "Pairing",
  "remote.pairing.expiresIn": "Expires in {secs}s. Scan the QR from the app or type the PIN.",
  "remote.pairing.cancel": "Cancel",
  "remote.pairing.start": "Pair new device",
  "remote.devices": "Paired devices ({count})",
  "remote.devices.empty": "None yet.",
  "remote.devices.pairedAgo": "paired {ago}",
  "remote.devices.seenAgo": "· seen {ago}",
  "remote.devices.revoke": "Revoke",
  "remote.devices.revokeConfirm": "Revoke this device? It will lose the connection.",
  "remote.time.secAgo": "{n}s ago",
  "remote.time.minAgo": "{n}m ago",
  "remote.time.hAgo": "{n}h ago",
  "remote.time.dAgo": "{n}d ago",

  "common.cancel": "Cancel",
  "common.save": "Save",
  "common.delete": "Delete",
  "common.loading": "Loading…",
  "common.empty": "Empty",
  "common.add": "Add",
  "common.remove": "Remove",
  "common.edit": "Edit",
  "common.close": "Close",
  "common.refresh": "Refresh",
  "common.connect": "Connect",
  "common.disconnect": "Disconnect",
  "common.connected": "Connected",
  "common.disconnected": "Disconnected",
  "common.universe": "Universe",
  "common.address": "Address",
  "common.channel": "Channel",
  "common.fixture": "Fixture",
  "common.fixtures": "Fixtures",
  "common.mode": "Mode",
  "common.name": "Name",
  "common.value": "Value",
  "common.enabled": "Enabled",
  "common.disabled": "Disabled",
  "common.optional": "optional",
  "common.yes": "Yes",
  "common.no": "No",

  "direct.title": "DMX Control — Direct Output (universe 0)",
  "direct.fpsNone": "— FPS",
  "direct.fpsValue": "{fps} FPS",
  "direct.statsMeta": "frames {frames} · late {late} · max {ms} ms",
  "direct.master": "Master",
  "direct.blackoutOn": "Blackout ON",
  "direct.blackout": "Blackout",
  "direct.clearAll": "Clear all",

  "library.title": "Library ({count})",
  "library.search": "Search…",
  "library.reload": "Reload",
  "library.uploadImage": "Upload image",
  "library.changeImage": "Change image",
  "library.errorOpenDialog": "Could not open dialog: {err}",
  "library.errorSetImage": "Could not change image: {err}",
  "library.modeSummary": "{name} · {count}ch",
  "library.filesPath": "JSON files live in {path}.",

  "outputs.title": "Outputs",
  "outputs.kind.mock": "Mock (logging)",
  "outputs.kind.artNet": "Art-Net",
  "outputs.kind.sacn": "sACN (E1.31)",
  "outputs.kind.enttec": "Enttec DMX USB Pro",
  "outputs.kind.openDmx": "Open DMX (OS serial — fallback)",
  "outputs.kind.openDmxFtdi": "Open DMX / ElectroTAS (FTDI direct, recommended)",
  "outputs.add.mock": "+ Mock",
  "outputs.add.artNet": "+ Art-Net",
  "outputs.add.sacn": "+ sACN",
  "outputs.add.enttec": "+ Enttec USB",
  "outputs.add.openDmx": "+ Open DMX (OS serial)",
  "outputs.add.openDmxFtdi": "+ Open DMX (FTDI direct)",
  "outputs.rescan": "Re-scan ports",
  "outputs.field.target": "Target IP:Port",
  "outputs.field.sourceName": "Source name",
  "outputs.field.priority": "Priority",
  "outputs.field.serialPort": "Serial port",
  "outputs.field.ftdiDevice": "FTDI device (by serial)",
  "outputs.field.dtrHigh": "DTR high",
  "outputs.field.rtsHigh": "RTS high",
  "outputs.field.universes": "Universes (comma-separated)",
  "outputs.placeholder.select": "— select —",
  "outputs.tag.ftdi": "(FTDI)",
  "outputs.tag.offline": "(offline)",
  "outputs.tag.inUse": "(in use)",
  "outputs.zadigHint":
    "On Windows, libusb only sees FTDI devices bound to WinUSB. If your DMX FTDI doesn't show up, run Zadig once and pick the WinUSB driver for that interface (this removes the COM port). To keep the COM port, switch this output to 'Serial' and pick the FTDI from the port list.",
  "outputs.remove": "Remove",
  "outputs.empty": "No outputs. Add one above.",
  "outputs.dirty": "Unsaved changes",
  "outputs.ok": "OK",
  "outputs.discard": "Discard",
  "outputs.apply": "Apply",

  "patch.title": "Patch ({count} fixtures)",
  "patch.statusClean": "✓ patch clean",
  "patch.statusIssues": "{conflicts} conflicts · {problems} problems",
  "patch.qty": "Qty",
  "patch.universe": "U",
  "patch.address": "Addr",
  "patch.labelPlaceholder": "Label (optional)",
  "patch.add": "Add",
  "patch.modeFmt": "{name} ({count}ch)",
  "patch.col.id": "ID",
  "patch.col.label": "Label",
  "patch.col.fixture": "Fixture",
  "patch.col.mode": "Mode",
  "patch.col.universe": "U",
  "patch.col.address": "Addr",
  "patch.col.range": "Range",
  "patch.del": "Del",
  "patch.unknownDef": "?? {id}",
  "patch.conflictFmt": "overlap on universe {u} channels {start}…{end}",

  "buttons.title": "Global buttons",
  "buttons.subhead": "Fades in milliseconds · independent in/out",
  "buttons.section.blackout": "Blackout",
  "buttons.section.blind": "Blind (halogen)",
  "buttons.blackoutIntro":
    "Cross-fades to off the channels you pick on each fixture. With no fixtures assigned = auto mode: every patched fixture kills intensity (or RGB if no dimmer) + strobe; pan/tilt/zoom stay put so heads don't slam to the floor. To target something specific (kill only intensity, or also a custom macro channel) add the fixtures below and tick the channels that should go to 0.",
  "buttons.blindIntro":
    "Hold-to-flash with a cross-fade against each light's current state. Hold the button: assigned fixtures jump in with a quick fade-in to halogen colour; on release, slow fade-out back to the colour they had (green at 50% returns to green). By default blind writes warm-white over intensity + RGB. To activate a specific channel instead (strobe, shutter, custom function), tick it on the fixture row — that channel slams to 255 instead of the default halogen.",
  "buttons.fadeIn": "Fade in (ms)",
  "buttons.fadeOut": "Fade out (ms)",
  "buttons.assignedBlackout": "Fixtures assigned to Blackout ({count})",
  "buttons.assignedBlackoutAuto": "Fixtures assigned to Blackout (auto · {count})",
  "buttons.assignedBlind": "Fixtures assigned to Blind ({count})",
  "buttons.emptyBlackout": "Patch fixtures first to assign them to blackout.",
  "buttons.emptyBlind": "Patch fixtures first to assign them to blind.",
  "buttons.blackoutHintAuto":
    "Auto: intensity (or RGB if not present) + strobe → 0. Pan/tilt untouched.",
  "buttons.blackoutHintCustom": "Only these channels go to 0.",
  "buttons.blindHintAuto": "Halogen default (warm-white over intensity + RGB).",
  "buttons.blindHintCustom": "Only these channels are slammed to 255 (no halogen).",

  "midi.title": "MIDI",
  "midi.intro":
    "Connect a control surface (e.g. Launchpad MK2). The first matching device is auto-connected at launch; you can override here.",
  "midi.devices": "Devices",
  "midi.refresh": "Refresh",
  "midi.connect": "Connect",
  "midi.disconnect": "Disconnect",
  "midi.connected": "Connected to {name}",
  "midi.notConnected": "Not connected.",
  "midi.lastEvent": "Last event: {event}",
  "midi.colorTest": "Color test",
  "midi.empty": "No MIDI devices found.",
  "midi.refreshHint":
    "Connect a MIDI controller (Launchpad MK2, etc.) to use as a surface. When you connect a Launchpad, the surface controller takes the bottom row (8 chasers, one per pad) and the scene buttons on the right side (red = blackout, white = momentary blind).",
  "midi.detectedDevices": "Detected devices ({count})",
  "midi.errListing": "Could not list MIDI devices: {err}",
  "midi.errConnect": "Could not connect to {name}: {err}",
  "midi.errDisconnect": "Could not disconnect: {err}",
  "midi.errTest": "Pad test failed: {err}",
  "midi.in": "in",
  "midi.out": "out",
  "midi.bothIO": "in + out",
  "midi.onlyIn": "in only",
  "midi.surfaceActive": "surface active",
  "midi.testPads": "Test pads (Launchpad MK2)",
  "midi.statusSection": "Status",
  "midi.connectedToFmt": "Connected to {name} ({io}){surface}",

  "ai.title": "AI",
  "ai.intro":
    "Provider, model and API key for LLM scene generation. Keys are kept in the OS app-config dir, off the show file.",
  "ai.provider": "Provider",
  "ai.provider.anthropic": "Anthropic (Claude)",
  "ai.provider.openai": "OpenAI",
  "ai.apiKey": "API key",
  "ai.apiKey.show": "Show",
  "ai.apiKey.hide": "Hide",
  "ai.model": "Model",
  "ai.test": "Test connection",
  "ai.testing": "Testing…",
  "ai.testOk": "Connection OK.",
  "ai.testFail": "Connection failed: {err}",
  "ai.save": "Save",
  "ai.saved": "Saved.",
  "ai.refreshModels": "Refresh models",
  "ai.subhead": "LLM scene generation (Anthropic / OpenAI) — POC",
  "ai.warning":
    "API keys are stored as plain text in the OS config dir (off the show file). In production they should move to the OS keychain.",
  "ai.providerActive": "Active provider",
  "ai.provider.none": "Disabled",
  "ai.provider.anthropicLong": "Anthropic (Claude)",
  "ai.provider.openaiLong": "OpenAI (GPT)",
  "ai.section.anthropic": "Anthropic",
  "ai.section.openai": "OpenAI",
  "ai.keyHint.anthropic": "Get an API key at",
  "ai.keyHint.openai": "Get an API key at",
  "ai.toggleKey.show": "Show key",
  "ai.toggleKey.hide": "Hide key",
  "ai.testDisabledHint": "Choose a provider and fill in the API key first",
  "ai.testActiveHint": "Make a minimal request to verify the API key + model",
  "ai.savedToast": "Configuration saved.",
  "ai.errPrefix": "Error: {err}",
};

export const es: Translation = {
  "app.brand": "DMX Control",
  "app.tab.stage": "Stage",
  "app.tab.scenes": "Escenas",
  "app.tab.chaser": "Chaser",
  "app.tab.movement": "Movimiento",
  "app.tab.preview3d": "Preview 3D",
  "app.tab.config": "Config",
  "app.openInOtherWindow": "Abrir {name} en otra ventana",
  "app.fullscreen": "Pantalla completa",
  "app.fullscreenHint": "Pantalla completa (F11)",
  "app.button.new": "Nuevo",
  "app.button.open": "Abrir…",
  "app.button.save": "Guardar",
  "app.global.blackout": "BLACKOUT",
  "app.global.blackoutTitle": "Blackout (toggle, fades configurables)",
  "app.global.blind": "BLIND",
  "app.global.blindTitle": "Blind / blinder (mantené presionado, halógeno con fade in/out)",
  "app.show.untitled": "Sin título",
  "app.show.unsaved": "(sin guardar) · click para renombrar",
  "app.show.renameHint": "{path} · click para renombrar",
  "app.show.renameHintEmpty": "(sin guardar) · click para renombrar",
  "app.dialog.closeTitle": "Cerrar aplicación",
  "app.dialog.closeBody":
    "¿Cerrar DMX Control?\nSe cerrarán también todas las ventanas popout abiertas.",
  "app.toast.savedAt": "Guardado en {path}",
  "app.toast.saveError": "Error al guardar: {err}",
  "app.toast.renameError": "No se pudo renombrar: {err}",

  "config.tabs.library": "Library",
  "config.tabs.libraryHint": "Definiciones de fixtures",
  "config.tabs.outputs": "Outputs",
  "config.tabs.outputsHint": "Universos y drivers DMX",
  "config.tabs.patch": "Patch",
  "config.tabs.patchHint": "Asignar fixtures a direcciones DMX",
  "config.tabs.blackoutBlind": "Blackout & Blind",
  "config.tabs.blackoutBlindHint": "Fades y fixtures afectados por los botones globales",
  "config.tabs.midi": "MIDI",
  "config.tabs.midiHint": "Superficie de control (Launchpad, etc.)",
  "config.tabs.ai": "IA",
  "config.tabs.aiHint": "Provider, modelo y API key para generación de escenas con LLM",
  "config.tabs.remote": "Remote",
  "config.tabs.remoteHint": "Bridge LAN para la app companion en iPhone",
  "config.tabs.sync": "Sync",
  "config.tabs.syncHint": "Compartir config entre máquinas via Gist privado de GitHub",
  "config.tabs.general": "General",
  "config.tabs.generalHint": "Idioma y otras preferencias",
  "config.tabs.direct": "Direct",
  "config.tabs.directHint": "Faders crudos por canal — herramienta de debug",

  "general.title": "General",
  "general.language": "Idioma",
  "general.languageHint": "Se aplica inmediatamente. Guardado por usuario en este equipo.",
  "general.language.en": "English",
  "general.language.es": "Español",
  "general.showFile": "Archivo de show",
  "general.showFileHint":
    "Crear un show nuevo, abrir uno del disco o guardar el actual en una ruta elegida.",

  "sync.title": "Sync",
  "sync.intro":
    "Compartí la config entre tus máquinas via Gist privado de GitHub. Cada usuario usa su propio PAT y su propio gist — nada compartido por defecto. Los Outputs se excluyen del push por defecto (el hardware suele diferir entre Mac y Windows).",
  "sync.section.token": "GitHub PAT",
  "sync.token.saved": "PAT guardado en el keychain.",
  "sync.token.delete": "Borrar",
  "sync.token.deleteConfirm": "¿Borrar el PAT del keychain?",
  "sync.token.placeholder": "ghp_… (scope: gist)",
  "sync.token.save": "Guardar",
  "sync.token.saving": "Guardando…",
  "sync.token.connectedAs": "Conectado como {user}.",
  "sync.token.help":
    "Generá un PAT en github.com/settings/tokens con el scope 'gist'. El token se guarda en el keychain del SO (macOS Keychain / Windows Credential Manager) — nunca toca un archivo.",
  "sync.section.settings": "Configuración",
  "sync.machineLabel": "Etiqueta de esta máquina",
  "sync.machineLabel.placeholder": "ej. mac-julio, windows-estudio",
  "sync.gistId": "Gist id",
  "sync.gistId.placeholder": "dejar vacío para que el primer Push cree uno",
  "sync.includeOutputs":
    "Incluir Outputs en el sync (sólo si las dos máquinas usan el mismo hardware)",
  "sync.saveSettings": "Guardar settings",
  "sync.section.status": "Estado",
  "sync.status.lastPush": "Última subida:",
  "sync.status.lastPull": "Última bajada:",
  "sync.status.lastRemote": "Última versión vista del remoto:",
  "sync.status.never": "nunca",
  "sync.status.from": "desde {machine}",
  "sync.probe": "Comprobar remoto",
  "sync.probing": "Comprobando…",
  "sync.probe.empty": "Remoto: vacío",
  "sync.probe.ahead": "Remoto: {time} · {machine} — adelantado, conviene Pull primero",
  "sync.probe.upToDate": "Remoto: {time} · {machine} — al día",
  "sync.section.actions": "Acciones",
  "sync.action.push": "Push",
  "sync.action.pushing": "Subiendo…",
  "sync.action.forcePush": "Force push",
  "sync.action.forcePushTitle": "Pisar el remoto sin chequear si está adelantado",
  "sync.action.pull": "Pull",
  "sync.action.pulling": "Bajando…",
  "sync.action.pullConfirm":
    "Pull va a sobreescribir el show actual con la versión del gist. ¿Seguir?",
  "sync.backupHint":
    "Antes de cada Pull la app guarda un backup del show actual en sync-backups/. El archivo más nuevo es tu show pre-pull.",
  "sync.toast.pushOk": "Push OK — gist {id}…, remoto {time}",
  "sync.toast.pullOk": "Pull OK — desde {machine}, remoto {time}. Backup en sync-backups/.",

  "remote.title": "Remote (mobile)",
  "remote.intro":
    "Bridge LAN para la app companion en iPhone. Sirve scenes / master / blackout / blind por WiFi local. Sin TLS — usar solo en redes confiables. En Windows el firewall suele pedir permiso la primera vez que arranca: aceptar al menos 'redes privadas'.",
  "remote.status": "Status:",
  "remote.statusRunning": "running",
  "remote.statusStopped": "stopped",
  "remote.start": "Start",
  "remote.stop": "Stop",
  "remote.connectManually": "Conectar manualmente:",
  "remote.clientsConnected": "Clientes conectados:",
  "remote.pairing": "Pairing",
  "remote.pairing.expiresIn": "Caduca en {secs}s. Escaneá el QR desde la app o ingresá el PIN.",
  "remote.pairing.cancel": "Cancelar",
  "remote.pairing.start": "Pair new device",
  "remote.devices": "Devices pareados ({count})",
  "remote.devices.empty": "Ninguno todavía.",
  "remote.devices.pairedAgo": "pareado {ago}",
  "remote.devices.seenAgo": "· visto {ago}",
  "remote.devices.revoke": "Revoke",
  "remote.devices.revokeConfirm": "¿Revocar este dispositivo? Va a perder la conexión.",
  "remote.time.secAgo": "hace {n}s",
  "remote.time.minAgo": "hace {n}m",
  "remote.time.hAgo": "hace {n}h",
  "remote.time.dAgo": "hace {n}d",

  "common.cancel": "Cancelar",
  "common.save": "Guardar",
  "common.delete": "Borrar",
  "common.loading": "Cargando…",
  "common.empty": "Vacío",
  "common.add": "Agregar",
  "common.remove": "Quitar",
  "common.edit": "Editar",
  "common.close": "Cerrar",
  "common.refresh": "Refrescar",
  "common.connect": "Conectar",
  "common.disconnect": "Desconectar",
  "common.connected": "Conectado",
  "common.disconnected": "Desconectado",
  "common.universe": "Universo",
  "common.address": "Dirección",
  "common.channel": "Canal",
  "common.fixture": "Fixture",
  "common.fixtures": "Fixtures",
  "common.mode": "Modo",
  "common.name": "Nombre",
  "common.value": "Valor",
  "common.enabled": "Habilitado",
  "common.disabled": "Deshabilitado",
  "common.optional": "opcional",
  "common.yes": "Sí",
  "common.no": "No",

  "direct.title": "DMX Control — Direct Output (universo 0)",
  "direct.fpsNone": "— FPS",
  "direct.fpsValue": "{fps} FPS",
  "direct.statsMeta": "frames {frames} · late {late} · máx {ms} ms",
  "direct.master": "Master",
  "direct.blackoutOn": "Blackout ON",
  "direct.blackout": "Blackout",
  "direct.clearAll": "Limpiar todo",

  "library.title": "Library ({count})",
  "library.search": "Buscar…",
  "library.reload": "Recargar",
  "library.uploadImage": "Subir imagen",
  "library.changeImage": "Cambiar imagen",
  "library.errorOpenDialog": "No se pudo abrir el diálogo: {err}",
  "library.errorSetImage": "No se pudo cambiar la imagen: {err}",
  "library.modeSummary": "{name} · {count}ch",
  "library.filesPath": "Los archivos JSON viven en {path}.",

  "outputs.title": "Outputs",
  "outputs.kind.mock": "Mock (logging)",
  "outputs.kind.artNet": "Art-Net",
  "outputs.kind.sacn": "sACN (E1.31)",
  "outputs.kind.enttec": "Enttec DMX USB Pro",
  "outputs.kind.openDmx": "Open DMX (OS serial — fallback)",
  "outputs.kind.openDmxFtdi": "Open DMX / ElectroTAS (FTDI directo, recomendado)",
  "outputs.add.mock": "+ Mock",
  "outputs.add.artNet": "+ Art-Net",
  "outputs.add.sacn": "+ sACN",
  "outputs.add.enttec": "+ Enttec USB",
  "outputs.add.openDmx": "+ Open DMX (OS serial)",
  "outputs.add.openDmxFtdi": "+ Open DMX (FTDI directo)",
  "outputs.rescan": "Re-escanear puertos",
  "outputs.field.target": "Target IP:Puerto",
  "outputs.field.sourceName": "Source name",
  "outputs.field.priority": "Prioridad",
  "outputs.field.serialPort": "Puerto serie",
  "outputs.field.ftdiDevice": "Dispositivo FTDI (por serial)",
  "outputs.field.dtrHigh": "DTR high",
  "outputs.field.rtsHigh": "RTS high",
  "outputs.field.universes": "Universos (separados por coma)",
  "outputs.placeholder.select": "— elegí —",
  "outputs.tag.ftdi": "(FTDI)",
  "outputs.tag.offline": "(offline)",
  "outputs.tag.inUse": "(en uso)",
  "outputs.zadigHint":
    "En Windows, libusb solo ve dispositivos FTDI bindeados a WinUSB. Si tu DMX FTDI no aparece, corré Zadig una vez y elegí el driver WinUSB para esa interface (eso saca el COM port). Si preferís mantener el COM port, cambiá esta salida a 'Serial' y elegí el FTDI desde la lista de puertos.",
  "outputs.remove": "Quitar",
  "outputs.empty": "Sin outputs. Agregá uno arriba.",
  "outputs.dirty": "Cambios sin aplicar",
  "outputs.ok": "OK",
  "outputs.discard": "Descartar",
  "outputs.apply": "Aplicar",

  "patch.title": "Patch ({count} fixtures)",
  "patch.statusClean": "✓ patch limpio",
  "patch.statusIssues": "{conflicts} conflictos · {problems} problemas",
  "patch.qty": "Qty",
  "patch.universe": "U",
  "patch.address": "Addr",
  "patch.labelPlaceholder": "Label (opcional)",
  "patch.add": "Add",
  "patch.modeFmt": "{name} ({count}ch)",
  "patch.col.id": "ID",
  "patch.col.label": "Label",
  "patch.col.fixture": "Fixture",
  "patch.col.mode": "Modo",
  "patch.col.universe": "U",
  "patch.col.address": "Addr",
  "patch.col.range": "Rango",
  "patch.del": "Del",
  "patch.unknownDef": "?? {id}",
  "patch.conflictFmt": "solapan en universo {u} canales {start}…{end}",

  "buttons.title": "Botones omnipresentes",
  "buttons.subhead": "Fades en milisegundos · in/out independientes",
  "buttons.section.blackout": "Blackout",
  "buttons.section.blind": "Blind (halógeno)",
  "buttons.blackoutIntro":
    "Apaga (con cross-fade) los canales que elijas de cada fixture. Sin fixtures asignados = modo automático: todos los fixtures patcheados apagan intensidad (o RGB si no tienen dimmer) + strobe; pan/tilt/zoom no se tocan para que los cabezales no salten al piso. Si querés algo específico (matar solo intensity, o también un canal de macro custom), agregá los fixtures abajo y tildá los canales que tienen que ir a 0.",
  "buttons.blindIntro":
    "Hold-to-flash con cross-fade contra el estado actual de cada luz. Mantené el botón: los fixtures asignados encienden con un fade-in rápido en color halógeno; al soltar, fade-out lento que vuelve al color que tenía antes (si era verde a 50%, vuelve al verde). Por defecto el blind escribe warm-white sobre intensity + RGB. Si querés que se active otro canal específico (strobe, shutter, función custom), tildalo en la fila del fixture y ese canal va a slamearse a 255 en lugar del halógeno default.",
  "buttons.fadeIn": "Fade in (ms)",
  "buttons.fadeOut": "Fade out (ms)",
  "buttons.assignedBlackout": "Fixtures asignados al Blackout ({count})",
  "buttons.assignedBlackoutAuto": "Fixtures asignados al Blackout (auto · {count})",
  "buttons.assignedBlind": "Fixtures asignados al Blind ({count})",
  "buttons.emptyBlackout": "Patcheá fixtures primero para poder asignarlos al blackout.",
  "buttons.emptyBlind": "Patcheá fixtures primero para poder asignarlos al blind.",
  "buttons.blackoutHintAuto":
    "Auto: intensity (o RGB si no hay) + strobe → 0. Pan/tilt intactos.",
  "buttons.blackoutHintCustom": "Solo estos canales se llevan a 0.",
  "buttons.blindHintAuto": "Halógeno default (warm-white sobre intensity + RGB).",
  "buttons.blindHintCustom": "Solo estos canales se mandan a 255 (sin halógeno).",

  "midi.title": "MIDI",
  "midi.intro":
    "Conectá una superficie de control (ej. Launchpad MK2). El primer dispositivo compatible se conecta al arranque; podés sobrescribirlo acá.",
  "midi.devices": "Dispositivos",
  "midi.refresh": "Refrescar",
  "midi.connect": "Conectar",
  "midi.disconnect": "Desconectar",
  "midi.connected": "Conectado a {name}",
  "midi.notConnected": "Sin conexión.",
  "midi.lastEvent": "Último evento: {event}",
  "midi.colorTest": "Test de color",
  "midi.empty": "No se encontraron dispositivos MIDI.",
  "midi.refreshHint":
    "Conectá un controlador MIDI (Launchpad MK2, etc.) para usarlo como surface. Cuando conectás un Launchpad, el surface controller toma la fila inferior (8 chasers, uno por pad) y los scene buttons del lateral derecho (rojo = blackout, blanco = blind momentáneo).",
  "midi.detectedDevices": "Dispositivos detectados ({count})",
  "midi.errListing": "No se pudo listar dispositivos MIDI: {err}",
  "midi.errConnect": "No se pudo conectar a {name}: {err}",
  "midi.errDisconnect": "No se pudo desconectar: {err}",
  "midi.errTest": "Falló el test de pads: {err}",
  "midi.in": "in",
  "midi.out": "out",
  "midi.bothIO": "in + out",
  "midi.onlyIn": "sólo in",
  "midi.surfaceActive": "surface activo",
  "midi.testPads": "Test pads (Launchpad MK2)",
  "midi.statusSection": "Estado",
  "midi.connectedToFmt": "Conectado a {name} ({io}){surface}",

  "ai.title": "IA",
  "ai.intro":
    "Provider, modelo y API key para generación de escenas con LLM. Las keys quedan en el dir de config del SO, fuera del show file.",
  "ai.provider": "Provider",
  "ai.provider.anthropic": "Anthropic (Claude)",
  "ai.provider.openai": "OpenAI",
  "ai.apiKey": "API key",
  "ai.apiKey.show": "Ver",
  "ai.apiKey.hide": "Ocultar",
  "ai.model": "Modelo",
  "ai.test": "Probar conexión",
  "ai.testing": "Probando…",
  "ai.testOk": "Conexión OK.",
  "ai.testFail": "Conexión falló: {err}",
  "ai.save": "Guardar",
  "ai.saved": "Guardado.",
  "ai.refreshModels": "Refrescar modelos",
  "ai.subhead": "Generación de escenas con LLM (Anthropic / OpenAI) — POC",
  "ai.warning":
    "Las API keys se guardan en texto plano en el directorio de configuración del SO (fuera del archivo del show). En producción deberían moverse al keychain del SO.",
  "ai.providerActive": "Provider activo",
  "ai.provider.none": "Desactivado",
  "ai.provider.anthropicLong": "Anthropic (Claude)",
  "ai.provider.openaiLong": "OpenAI (GPT)",
  "ai.section.anthropic": "Anthropic",
  "ai.section.openai": "OpenAI",
  "ai.keyHint.anthropic": "Conseguí una API key en",
  "ai.keyHint.openai": "Conseguí una API key en",
  "ai.toggleKey.show": "Mostrar key",
  "ai.toggleKey.hide": "Ocultar key",
  "ai.testDisabledHint": "Elegí un provider y completá la API key primero",
  "ai.testActiveHint": "Hacer un request mínimo para verificar la API key + modelo",
  "ai.savedToast": "Configuración guardada.",
  "ai.errPrefix": "Error: {err}",
};

export type LanguageCode = "en" | "es";

export const LANGUAGES: { code: LanguageCode; nativeName: string }[] = [
  { code: "en", nativeName: "English" },
  { code: "es", nativeName: "Español" },
];

export const DICTIONARIES: Record<LanguageCode, Translation> = { en, es };
