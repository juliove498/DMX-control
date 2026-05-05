# DMX Remote — Companion iPhone App

## Context

El proyecto desktop (Tauri 2 + Rust + React) controla rigs DMX y ya tiene ~55 comandos cubriendo escenas, fixtures, programmer, chasers, movements y globals. Hoy todo el control vive en la PC. El operador necesita mover scenes, master y blackout desde el iPhone mientras camina por la sala — el caso típico de busking. Este plan diseña un proyecto NUEVO y separado (`dmx-remote/`) en React Native + Expo y la capa bridge que falta del lado del desktop para que el iPhone se conecte por WiFi local.

**Decisiones que enmarcan el plan:**
- Repo separado (`dmx-remote/`), bindings sincronizados con rsync.
- LAN-only en v1 pero diseñando el protocolo para que un relay cloud futuro funcione sin rediseño.
- MVP mínimo: pairing + scenes (GO/RELEASE) + master + blackout. Chasers/programmer/faders en Phase 2.

## Arquitectura

```
   iPhone (Expo)                       Desktop (Tauri)
 ┌──────────────────┐  WiFi LAN      ┌────────────────────────┐
 │ Connect screen   │ ───mDNS────►   │ bridge::discovery       │
 │ Scenes screen    │ ◄──pair PIN──  │ bridge::auth (PIN→tok)  │
 │ Globals screen   │ ◄══════JSON-RPC over WS════►            │
 │  ConnectionMgr   │                │ bridge::server (axum)   │
 │  (Zustand+RQ)    │                │ bridge::rpc (whitelist) │
 └──────────────────┘                │ bridge::events (tee)    │
                                     │     ↓ ↑                 │
                                     │ Tauri command bus       │
                                     │ Engine / Programmer /   │
                                     │ Scenes / Globals (∗)    │
                                     └────────────────────────┘
                                       (∗) reused as-is
```

**Bridge embebido dentro de Tauri** (no proceso separado). El bridge no reimplementa lógica: recibe un JSON-RPC, hace match contra una whitelist de métodos, y llama internamente a las mismas funciones Rust que ya usan los `#[tauri::command]`. Eventos de Tauri (`show:updated`, `engine:stats`, los nuevos) se tee'an a todos los WS abiertos. Mismo modelo que ya usan los popouts, extendido a la red.

**Transporte:** JSON-RPC 2.0 sobre un único WebSocket por cliente, más dos endpoints REST mínimos para arrancar (`POST /pair`, `GET /health`). Misma trama de mensajes funciona idéntica si en el futuro la conexión va por relay cloud — solo cambia la URL de destino.

**Discovery + pairing:**
1. Desktop publica `_dmxctrl._tcp.local.` por mDNS (puerto, nombre, fingerprint).
2. iPhone usa `react-native-zeroconf` para listar bridges en la red.
3. Usuario abre "Pair new device" en desktop → PIN de 6 dígitos + QR (60 s de ventana).
4. iPhone escanea QR (expo-camera) o tipea PIN → `POST /pair {pin}` → server devuelve `{token, deviceName}` y guarda hash en `~/.config/dmx-control/bridge/devices.json`.
5. WS connect con `Authorization: Bearer <token>` → comparación constant-time del SHA-256.

**Sin TLS en v1.** Bind a `0.0.0.0` pero rechazo conexiones cuyo peer IP no esté en rangos RFC1918/link-local salvo que el usuario habilite explícitamente. Token revocable desde el desktop.

**State sync:** al conectar, server emite `initial_state` con snapshot completo (`Show`, `EngineStats`, `ProgrammerStatus`, `active_scene_id`). Después solo deltas. Esto **mata el polling de 200 ms** que hoy hace el frontend desktop en [src/components/ScenesView.tsx](../src/components/ScenesView.tsx) — los nuevos eventos también los puede consumir el desktop.

## Cambios Phase 1 — Desktop side

### Nuevo módulo `src-tauri/src/bridge/`

| Archivo | Responsabilidad |
|---|---|
| `mod.rs` | Init, lifecycle, exporta tauri commands `bridge_*` |
| `server.rs` | axum server, rutas REST + upgrade WS |
| `rpc.rs` | Dispatcher JSON-RPC, **whitelist de métodos** (deny por default) |
| `auth.rs` | PIN gen, token issue, devices.json store, constant-time compare |
| `discovery.rs` | mdns-sd advertise/stop |
| `events.rs` | Listen Tauri events → broadcast a WS clients (con throttle p/ engine:stats) |

### Métodos expuestos por el RPC en Phase 1 (whitelist)

```rust
// scenes
"list_scenes"        → existing list_scenes()
"recall_scene"       → existing recall_scene(scene_id, fade_ms)
"release_scene"      → existing release_scene()
"active_scene_id"    → existing active_scene_id()
"active_scene_step"  → existing active_scene_step()
// globals
"get_globals"        → existing get_globals()
"set_master"         → existing set_master(value)
"set_blackout"       → existing set_blackout(on)
"set_blind"          → existing set_blind(pressed)
// snapshot
"get_show"           → existing get_show()
```

**Explícitamente NO expuestos en Phase 1** (footguns desde el green room): `set_outputs`, `*_fixture`, `delete_*`, `update_scene`, `add_scene_step`, todos los `ai_*`, `set_channel`, `set_fixture_channel`, `clear_universe`, `new_show`/`open_show`/`save_show`. La whitelist es un `match` literal, no un atributo en cada comando — los `#[tauri::command]` quedan intactos.

### Nuevos eventos Tauri (también consumibles por el desktop frontend)

Emitidos desde los mismos sitios que ya mutan el estado:
- `programmer:changed` → `ProgrammerStatus` — emitido en [src-tauri/src/programmer.rs](../src-tauri/src/programmer.rs) en touch/untouch/clear
- `scene:active_changed` → `{ active_scene_id: Option<String>, step_index: Option<u32> }` — desde `recall_scene_impl` y `release_scene` en [src-tauri/src/commands.rs](../src-tauri/src/commands.rs)
- `engine:master_changed` → `{ master: u8 }` — desde `set_master`

Esto permite reemplazar el polling 200 ms por suscripción push tanto en mobile como en el frontend desktop existente. **La migración del frontend desktop NO es parte de Phase 1**, queda como nota; solo se agregan los eventos.

### Nuevos `#[tauri::command]` para la UI de bridge en desktop

```
bridge_start, bridge_stop, bridge_status,
bridge_begin_pairing, bridge_cancel_pairing,
bridge_list_devices, bridge_revoke_device
```

### Nuevos tipos ts-rs (auto-bindings)

`BridgeStatus`, `PairedDevice`, `PairingState`, `BridgeEvent` (tagged union de los eventos que viajan por WS).

### UI de bridge en desktop

Nueva tab "Remote" en [src/components/ConfigView.tsx](../src/components/ConfigView.tsx) (grupo Surface, junto a "IA"):
- Toggle Start/Stop bridge
- IP + puerto detectados, status (running / idle / error)
- Botón "Pair new device" → modal con PIN grande + QR (60 s countdown)
- Lista de devices pareados con botón Revoke
- Indicador de clientes conectados ahora

### Cargo deps a agregar a [src-tauri/Cargo.toml](../src-tauri/Cargo.toml)

```toml
axum = "0.7"
tokio-tungstenite = "0.23"
tower-http = { version = "0.5", features = ["cors"] }
mdns-sd = "0.11"
sha2 = "0.10"
rand = "0.8"
qrcodegen = "1.8"   # para generar el QR del PIN
```

### Capabilities

[src-tauri/capabilities/default.json](../src-tauri/capabilities/default.json) — agregar permisos para los nuevos `bridge_*` commands.

## Estructura del nuevo proyecto `dmx-remote/`

Repo separado, al lado del proyecto actual. Expo SDK 51, RN 0.74, TypeScript estricto.

```
dmx-remote/
  app.json
  package.json
  tsconfig.json
  app/                         expo-router
    _layout.tsx                root + ConnectionProvider
    connect.tsx                discovery + pairing wizard
    (tabs)/
      _layout.tsx              tab bar (Scenes | Globals | Connect)
      scenes.tsx
      globals.tsx
  src/
    bridge/
      ConnectionManager.ts     WS singleton, auto-reconnect, JSON-RPC client
      rpc.ts                   typed call<T>(method, params), event subscription
      discovery.ts             react-native-zeroconf wrapper
      auth.ts                  expo-secure-store wrapper p/ token
    stores/
      connection.ts            Zustand: status, deviceName, lastError
      engine.ts                Zustand: master, blackout, activeSceneId, activeSceneStep
      show.ts                  Zustand: scenes list (hidratado del snapshot)
    queries/
      useSnapshot.ts           React Query: GET initial show snapshot
      mutations.ts             recallScene, releaseScene, setMaster, setBlackout
    components/
      SceneTile.tsx
      MasterFader.tsx
      BlackoutButton.tsx
      ConnectionBadge.tsx
    types/
      generated/               ← rsync target de src-tauri/bindings/
  scripts/
    sync-bindings.sh           rsync ../DMX-control-project/src-tauri/bindings/* → src/types/generated/
```

**Deps clave:** `expo-router`, `expo-secure-store`, `expo-camera`, `react-native-zeroconf`, `zustand`, `@tanstack/react-query`.

**Por qué Zustand + React Query:** snapshot inicial y mutations son request/response (RQ los maneja con retry + cache). Estado live (master, blackout, active scene) llega por push y es propiedad del `ConnectionManager` → Zustand. Sin optimismo en mutations: el round-trip por WS local es < 100 ms y la consistencia del estado emitido por el bridge es la verdad.

**Sharing types:** `scripts/sync-bindings.sh` con un rsync simple. Sin npm package, sin submodule. Se corre antes de cada build mobile.

## Diseño "futuro relay-friendly"

El protocolo JSON-RPC sobre WS es agnóstico al transporte. Para soportar cloud relay después sin rediseño:
- El cliente abstrae la URL en `ConnectionManager.connect(target: { kind: "lan", ip, port } | { kind: "relay", url, room })`.
- Auth: el bearer token actual sigue valiendo; un relay solo lo reenviaría.
- Eventos server-side están tipados como `BridgeEvent` (tagged union) — un relay puede repartirlos por room sin entender semántica.

Lo que **no** se hace en Phase 1: TLS, certs, rate limiting global, el relay en sí. Solo se evita pintarse en una esquina.

## Riesgos a validar antes de codear

- **Permiso "Local Network" en iOS 14+:** `react-native-zeroconf` lo dispara. La pantalla Connect tiene que explicar el por qué, o el usuario lo niega y queda sin discovery. Fallback manual con IP+puerto siempre disponible.
- **mdns-sd en Windows / mDNSResponder en Mac:** en Mac viene de fábrica; en Windows no siempre. Ya que vas a desarrollar en Mac, esto es viento de cola. Igual: manual-IP siempre disponible como fallback.
- **AP isolation en redes guest:** muchos venues lo tienen. Manual IP como fallback no negociable.
- **Tauri `app.emit` clona payload por webview:** medir el costo del tee a WS clients con `engine:stats` 15 Hz × N clientes (probable que sea negligible, pero confirmar).
- **iOS suspende WS al backgroundear rápido:** auto-reconnect con backoff (250 ms → 8 s, jitter) + foreground-trigger en `AppState` listener. Status visible siempre en el header.
- **Sin TLS:** documentar en README que es LAN-only en v1; no bind a interfaz WAN sin toggle explícito.

## Verificación end-to-end del MVP

1. **Build & start desktop:** `cd DMX-control-project && npm run tauri dev`. Abrir tab "Remote" en Config, click Start. Verificar log "bridge listening on 0.0.0.0:7878" y "mDNS advertising _dmxctrl._tcp.local.".
2. **Build & start mobile:** `cd dmx-remote && npm install && npx expo start`. Abrir en Expo Go en el iPhone (misma WiFi).
3. **Discovery:** abrir Connect. Tiene que aparecer el desktop por mDNS. Si no, ingresar IP manual.
4. **Pairing:** click "Pair" en mobile → desktop muestra PIN + QR. Escanear QR. Token guardado en Keychain (verificar persiste tras kill app).
5. **WS subscribe:** ConnectionBadge en verde, Scenes screen lista escenas del show actual.
6. **Recall:** tap GO en una escena → fixture en rig responde, badge "Active: <name>" aparece en mobile, mismo update aparece en el desktop sin polling.
7. **Master:** mover slider en mobile → master en desktop responde de inmediato y vice-versa (eventos van en ambas direcciones).
8. **Blackout:** botón blackout en mobile → DMX out cae a 0, indicador rojo en mobile y desktop sincronizado.
9. **Reconnect:** apagar WiFi del iPhone 30 s, prender → ConnectionBadge cicla connecting → connected, snapshot se refetch, estado consistente.
10. **Multi-client:** abrir un popout del desktop + mobile + segunda instancia del mobile. Disparar GO desde cualquiera → los tres reflejan el cambio.
11. **Revoke:** revoke el device desde desktop → mobile recibe close 4001 (auth revoked) y vuelve a Connect screen.

Tests automáticos del bridge: unit tests del dispatcher RPC (whitelist enforcement, payload shape) y del PIN/token flow (constant-time compare, expiración). E2E mobile queda manual en este POC.

## Phase 2 (out of scope ahora, para referencia)

- Programmer screen (status push + untouch)
- Chasers/movements toggles
- Blind toggle dedicado
- Migrar polling 200 ms del frontend desktop a los eventos push nuevos
- Pantalla "Devices" en mobile p/ ver / olvidar bridges pareados
- Live faders con coalescing 30 Hz / last-value-wins / back-pressure check
- Haptics en GO / blackout

## Critical files

**A modificar:**
- [src-tauri/Cargo.toml](../src-tauri/Cargo.toml) — deps nuevas
- [src-tauri/src/lib.rs](../src-tauri/src/lib.rs) — registrar el módulo bridge y los nuevos commands
- [src-tauri/src/commands.rs](../src-tauri/src/commands.rs) — emitir `scene:active_changed` desde `recall_scene_impl` y `release_scene`; `engine:master_changed` desde `set_master`
- [src-tauri/src/programmer.rs](../src-tauri/src/programmer.rs) — emitir `programmer:changed` en touch / untouch / clear
- [src-tauri/capabilities/default.json](../src-tauri/capabilities/default.json) — permisos nuevos commands
- [src/components/ConfigView.tsx](../src/components/ConfigView.tsx) — tab "Remote"

**A crear (desktop):**
- `src-tauri/src/bridge/mod.rs`
- `src-tauri/src/bridge/server.rs`
- `src-tauri/src/bridge/rpc.rs`
- `src-tauri/src/bridge/auth.rs`
- `src-tauri/src/bridge/discovery.rs`
- `src-tauri/src/bridge/events.rs`
- `src/components/RemoteBridgeView.tsx`

**A crear (proyecto nuevo `dmx-remote/`):** estructura completa listada arriba.

## Notas para retomar en Mac

- En Mac, `mdns-sd` no necesita Bonjour service extra (viene del sistema). Discovery debería funcionar out-of-the-box.
- `rsync` viene de fábrica → `scripts/sync-bindings.sh` funciona sin instalar nada.
- Expo Go en iPhone se conecta directo si Mac y phone están en la misma WiFi — no hay drama de WSL ni firewall de Windows.
- Para empezar: crear `dmx-remote/` al lado de `DMX-control-project/`, `npx create-expo-app@latest dmx-remote --template`, configurar tsconfig, agregar deps, copiar `scripts/sync-bindings.sh`.
- Primer paso real de código: el módulo `src-tauri/src/bridge/` con un `/health` REST que responda OK y un WS de echo. Validar que el iPhone se puede conectar antes de meterle protocolo arriba.
