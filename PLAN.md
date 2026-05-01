# Plan de desarrollo: DMX Controller (React + Tauri + Rust)

> Documento de referencia para sesiones con Claude Code.
> Objetivo: software de control DMX para uso propio en shows reales.
> Prioridades por orden: confiabilidad > latencia predecible > features.

---

## Stack

- **Frontend**: React + TypeScript + Vite
- **Backend**: Rust (vía Tauri 2.x)
- **Estado UI**: Zustand
- **Estado del show**: vive en Rust, React es solo vista
- **IPC**: Tauri commands (UI → Rust) + eventos (Rust → UI)
- **Persistencia**: archivos JSON del show + autosave

## Principios de diseño no negociables

1. El thread de output DMX **nunca** debe bloquearse por la UI.
2. El estado del show es la fuente de verdad en Rust, no en React.
3. Toda mutación de estado pasa por un comando Tauri que retorna `Result`.
4. Autosave en cada mutación. Mínimo 3 backups rotativos.
5. Mock output mode siempre disponible para desarrollar sin hardware.
6. Logs persistentes con `tracing` desde el día 1.

---

## Fase 0 — Setup del proyecto (1-2 días)

### Objetivo
Repositorio funcional con React + Tauri compilando, hot reload, estructura clara.

### Tareas
- [ ] `npm create tauri-app@latest` con template React + TypeScript
- [ ] Configurar ESLint + Prettier + Biome (elegir uno)
- [ ] Estructura de carpetas:
  ```
  src/                    # React
    components/
    stores/               # Zustand
    types/                # tipos compartidos con Rust (generar con ts-rs)
  src-tauri/
    src/
      engine/             # core del motor DMX
      output/             # drivers (artnet, sacn, enttec)
      show/               # modelo del show, persistencia
      commands.rs         # comandos expuestos a React
      lib.rs
    Cargo.toml
  ```
- [ ] Setup de `tracing` + `tracing-subscriber` con archivo de log rotativo
- [ ] Crate `ts-rs` para generar tipos TypeScript desde structs Rust
- [ ] Script de build que verifique que tipos TS están sincronizados
- [ ] CI básico: `cargo test`, `cargo clippy`, `npm run build`

### Criterio de done
- App vacía abre, hot reload anda, logs se escriben en disco.

---

## Fase 1 — Output DMX confiable (2-4 semanas)

### Objetivo
Sacar 512 canales por Art-Net a 44Hz constante, sin jitter perceptible, con UI mínima de verificación.

### Tareas

**Core engine (Rust)**
- [ ] Struct `Universe` con `[u8; 512]` y un número de universo
- [ ] Tipo `EngineState` que contiene N universos en `Arc<RwLock<...>>`
- [ ] Thread dedicado `OutputThread` que corre a 44Hz (loop con `tokio::time::interval` o `std::thread::sleep_until`)
- [ ] El thread lee snapshot del estado, lo manda al output driver
- [ ] Watchdog: si el loop tarda más de 30ms, log de warning
- [ ] Comando Tauri `set_channel(universe, channel, value)` para tests

**Output driver Art-Net**
- [ ] Implementar trait `OutputDriver { fn send(&mut self, universe: u16, data: &[u8; 512]) }`
- [ ] Driver Art-Net: socket UDP, paquete ArtDMX (opcode 0x5000)
- [ ] Test de integración con receptor (QLC+ en otra máquina o nodo real)
- [ ] Configuración: IP destino, universo

**UI de verificación**
- [ ] Vista "Direct Output" con 512 sliders (virtualizada, react-window)
- [ ] Slider master arriba que multiplica todos
- [ ] Botón "blackout" que pone todo en 0
- [ ] Indicador de FPS real del thread de output (evento desde Rust cada segundo)

### Criterio de done
- Mover un slider en React mueve la luz en menos de 50ms.
- Engine corre 30 minutos sin perder un solo frame.
- Cerrar la app no deja luces colgadas (último frame en 0 antes de cerrar).

### Notas para Claude Code
- Empezar con un test que verifique el formato de paquete Art-Net contra un dump conocido.
- No usar `async` para el loop de output. Thread normal con `Instant::now()` y `sleep_until`. Async tiene jitter por el scheduler.

---

## Fase 2 — sACN y Enttec USB Pro (1-2 semanas)

### Objetivo
Tres drivers de salida intercambiables. UI para configurar cuál se usa.

### Tareas
- [ ] Driver sACN (E1.31): UDP multicast a 239.255.x.x, paquete con prioridad
- [ ] Driver Enttec DMX USB Pro: crate `serialport`, mensaje 0x06 con label
- [ ] Detección de dispositivo Enttec (enumerar puertos serie, filtrar por VID/PID FTDI)
- [ ] Config persistente: lista de "outputs" (cada uno con tipo + parámetros + universo asignado)
- [ ] UI de configuración de outputs (agregar, quitar, editar)
- [ ] Mock driver que solo loggea, para desarrollo sin hardware

### Criterio de done
- Un mismo show sale por Art-Net y Enttec simultáneamente sin diferencias visibles.
- Desconectar el USB no crashea, log de error y reintento de conexión cada 2 segundos.

---

## Fase 3 — Modelo de fixtures y patch (3-4 semanas)

### Objetivo
Cargar definiciones de fixtures, hacer patch al universo, ver fixtures en una vista 2D.

### Tareas

**Modelo**
- [ ] Tipo `FixtureDefinition` con channels tipados (Intensity, ColorR/G/B, Pan, Tilt, etc.)
- [ ] Formato JSON propio para fixtures (ejemplo en `/fixtures/library/`)
- [ ] Tipo `FixtureInstance`: definición + dirección DMX + universo + ID + posición XY
- [ ] Validador de patch: detectar overlaps de canales

**UI**
- [ ] Librería de fixtures: lista navegable con búsqueda
- [ ] Editor de fixture (crear/editar definiciones)
- [ ] Vista "Patch": tabla con fixture, universo, dirección, modo
- [ ] Vista "Stage": grilla 2D para posicionar fixtures (drag&drop con dnd-kit)
- [ ] Importar fixtures desde QLC+ (`.qxf` es XML simple, parser opcional)

**Persistencia**
- [ ] Show file format v1 (JSON con versionado para migraciones futuras)
- [ ] Autosave cada cambio + 3 backups rotativos (.bak.1, .bak.2, .bak.3)

### Criterio de done
- Patch de 20 fixtures, guardar, cerrar app, reabrir, todo está.
- Cambiar un canal de un fixture en la UI saca el valor correcto en el universo correcto.

---

## Fase 4 — Programador y escenas (4-6 semanas)

### Esta fase es la más importante. No apurarla.

### Objetivo
Modelo de programador estilo Avolites/Chamsys: seleccionás fixtures, modificás atributos, grabás como cue.

### Tareas

**Engine**
- [ ] Tipo `Programmer`: lista de valores activos `(fixture_id, attribute) -> value`
- [ ] Sistema de "touched" attributes: solo lo que tocaste se graba
- [ ] Tipo `Cue`: snapshot de programmer + tiempos (fade in, fade out, delay)
- [ ] Motor de merge: programmer pisa playback (LTP por defecto, HTP para intensidad opcional)
- [ ] Engine renderiza cada frame: estado base → cuelists → programmer → output

**UI**
- [ ] Selección de fixtures (click, shift+click, grupos)
- [ ] Encoder wheels virtuales para atributos (intensidad, pan, tilt, color)
- [ ] Color picker que mapea a canales R/G/B (o CMY)
- [ ] Position grid para pan/tilt
- [ ] Botón "Record" → graba programmer como cue/escena
- [ ] Botón "Clear" → vacía programmer
- [ ] Lista de escenas con preview

### Criterio de done
- Crear 5 escenas, dispararlas con fade, ver transiciones suaves.
- Programmer pisa cualquier escena activa.
- Clear devuelve control a las escenas.

### Notas para Claude Code
- El motor de merge es el corazón. Diseñarlo con tests unitarios desde el principio.
- Cada layer (base, cuelists, programmer) produce un `HashMap<(FixtureId, AttrId), Value>` que se mergea en orden.
- Los fades son interpolación entre snapshot anterior y nuevo, con curva (lineal al principio, S-curve después).

---

## Fase 5 — Cuelists y submasters (3-4 semanas)

### Objetivo
Listas de cues con Go/Back, faders que controlan intensidad de cuelists.

### Tareas
- [ ] Tipo `Cuelist`: lista ordenada de cues + estado (current, next)
- [ ] Comandos: `go(cuelist_id)`, `back(cuelist_id)`, `release(cuelist_id)`
- [ ] Follow times (auto-go después de N segundos)
- [ ] Submasters: fader 0-100% que escala intensidad de un cuelist
- [ ] UI: vista de cuelists con botón Go grande, lista de cues
- [ ] UI: bank de submasters (10-20 faders)
- [ ] Atajos de teclado configurables (Go = Space, etc.)

### Criterio de done
- Cuelist de 10 cues con tiempos diferentes corre solo con follows.
- Submaster baja un cuelist sin afectar otros.

---

## Fase 6 — Efectos generadores (4+ semanas)

### Objetivo
Aplicar formas de onda (sine, cosine, ramp, square) a atributos con offset por fixture.

### Tareas
- [ ] Tipo `Effect`: forma + atributo destino + size + speed + offset por fixture
- [ ] Engine de efectos corre dentro del engine principal
- [ ] Efectos se graban en cues como cualquier otro valor
- [ ] UI: editor de efectos con preview en vivo
- [ ] Presets: circle (pan/tilt sine + cosine), color chase, dimmer chase

### Criterio de done
- Circle effect en 4 movers se ve como círculo real.
- Speed y size editables en vivo sin glitches.

---

## Fase 7 — Hardening para shows reales (continuo)

### Estas tareas se hacen en paralelo con todo lo anterior, pero acá se formalizan.

### Tareas
- [ ] Modo "show": bloquea edición, solo playback, reduce riesgo de toques accidentales
- [ ] Autosave + 3 backups verificado con tests
- [ ] Recovery: al abrir, si el último cierre fue crash, ofrecer restaurar último estado
- [ ] DMX hold on close (configurable)
- [ ] Watchdog del engine con auto-restart
- [ ] Logs estructurados con niveles, archivo rotado por día
- [ ] Pantalla de "diagnóstico": FPS real, latencia IPC, drivers conectados, errores recientes
- [ ] Export/import de show files
- [ ] MIDI in para controladoras (crate `midir`), mapeo de notas/CC a comandos

---

## Convenciones para sesiones con Claude Code

### Cómo arrancar cada sesión
1. Pegar referencia a este PLAN.md y a la fase actual.
2. Decir qué tarea específica de la fase se está atacando.
3. Pedir tests primero cuando aplique (engine, parsers, drivers).

### Cómo trabajar
- Cambios chicos y verificables. Un commit por tarea del checklist.
- Al tocar algo del engine, correr la suite completa.
- Al tocar drivers de output, probar con mock + hardware real.

### Qué NO hacer
- No mover estado del show a React.
- No usar `async` en el loop de output DMX.
- No agregar dependencias pesadas sin discutir.
- No saltar fases. La fase 1 anda perfecta antes de empezar la 2.

---

## Hardware mínimo para desarrollar

- Una interfaz Art-Net (un nodo barato o software como QLC+ corriendo de receptor).
- Idealmente una Enttec Open DMX o USB Pro.
- 2-3 fixtures DMX (un par LED RGB y un mover chico alcanza para todo).
- En el peor caso, solo el mock driver + visualizer 3D después.

---

## Decisiones que dejo abiertas para más adelante

- Visualizer 3D (three.js / react-three-fiber): post-fase 6.
- Soporte GDTF: cuando la librería de fixtures propia se quede chica.
- Timecode (LTC/MTC): si algún show lo requiere.
- Multi-window (programador en pantalla aparte): post-fase 5.
- Networking entre dos instancias (backup tracking): muy avanzado, no antes de un año.
