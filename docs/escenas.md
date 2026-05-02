# Manual de Escenas

Guía práctica para usar la feature de Escenas (Phase 4 del plan). Cubre el
flujo completo: grabar el rig en un snapshot, recallearlo con fade, editar
sin perder el resto del show, y disparar todo desde el Launchpad MK2.

> **TL;DR**: una **escena** es una foto del estado de tus fixtures. La
> grabás, le ponés un fade, la recalleás cuando quieras y el rig se mueve
> suave hasta esa foto. La fila 3 del Launchpad ejecuta las primeras 8.

---

## Conceptos

**Escena**
: Snapshot guardado de un set de fixtures con sus valores DMX y un fade in
  configurable. Vive dentro del archivo del show (`.json`), así que viaja
  con el archivo cuando lo copiás a otra máquina.

**Recall (▶ GO)**
: Disparar una escena. El motor anima el rig desde donde está actualmente
  hasta los valores grabados, durante el `fade_in_ms` configurado. Si
  pisás GO en otra escena a mitad del fade, el nuevo fade arranca desde el
  valor instantáneo (no hay snap-back).

**Programmer (touched-tracking)**
: Tracker liviano que recuerda qué fixtures tocaste desde el último Clear
  o Record. Sirve para grabar/actualizar solo los fixtures que estabas
  modificando, sin pisar el resto de la escena.

**Active scene**
: La última escena recalleada. Queda marcada como activa (banda dorada en
  la fila + pad parpadeando en el Launchpad) hasta que recallés otra o
  presionés "Liberar".

---

## Flujo básico (3 pasos)

### 1. Armar el look en Stage

Andá a la pestaña **Stage**, seleccioná los fixtures (click, arrastre, o
ctrl/cmd-click) y modificá sus canales: intensidad, color, pan/tilt,
strobe, lo que necesites.

Cada vez que movés un slider o tocás un botón de range, el fixture queda
marcado como **touched** en el programmer. Vas a ver:

- En la pestaña Scenes: una **barra violeta "PROG"** abajo del bloque
  grabar con el conteo (`PROG · 3 fixtures tocados`).
- En la grilla de chips: punto violeta al lado del nombre.

### 2. Grabar la escena

Andá a la pestaña **Scenes**:

1. Escribí un nombre (`"Punto azul intro"`, `"Strobo coro 1"`, `"Apagón
   total"`...). Si lo dejás vacío, el sistema usa `Scene N` con el
   próximo número libre.
2. Configurá el **fade in (ms)**. Algunos puntos de referencia:
   - `0`: snap inmediato. Útil para cortes secos en transición de tema.
   - `300-500`: cambio rápido pero suave. Buena base por default.
   - `1500-3000`: cross-fade musical entre looks.
   - `8000+`: ambient build (intro, outros largas).
3. Elegí qué fixtures captura. Tres caminos:
   - **Todos**: graba el rig completo. Útil para "instantánea total" como
     primer cue de un tema.
   - **Selección manual**: tildá los chips de los fixtures que querés.
   - **Touched (N)**: pre-selecciona los fixtures que el programmer
     marcó como tocados. Click rápido para grabar solo lo que recién
     editaste.
   - O activá el toggle **"Grabar solo los touched"** y el botón Record
     ignora la grilla y usa directamente el set tocado.
4. **Record** (botón naranja). La escena aparece en la lista de abajo.

### 3. Recallarla

Click **▶ GO** en la fila de la escena. El rig fadea suave hasta los
valores grabados. La fila se pone con borde dorado mientras está activa,
y aparece la barra "Escena activa: ..." con un botón **Liberar** que
deja el motor en el estado actual sin volver atrás.

Recall también se dispara desde el **Launchpad row 3** (notes 31-38) →
ver sección Launchpad abajo.

---

## Editar una escena sin perder lo demás

El caso clásico: tenés una escena con 12 fixtures, querés cambiar el
color de **2 movers** sin tocar los otros 10.

### Camino A — Update Touched (recomendado)

1. Recallá la escena (▶ GO).
2. Volvé a Stage, modificá solo los 2 movers.
3. Volvé a Scenes — vas a ver "PROG · 2 fixtures tocados".
4. En la fila de la escena, click **⟳ Touched**.

Solo esos 2 movers se re-graban con el estado actual. Los otros 10
quedan exactamente como estaban grabados antes. Es el flujo Avolites
clásico.

### Camino B — Update completo

Si querés re-grabar **todos los fixtures** de la escena con el estado
actual (típico cuando armaste el look desde cero y querés sobreescribir):

1. Andá a Stage, armá el look.
2. En la fila de la escena, click **⟳ Update**.

Re-graba la totalidad de los fixtures que la escena ya contenía,
manteniendo el conjunto (no agrega ni saca fixtures de la escena, solo
refresca sus valores).

### Camino C — Edit nombre / fade

Click en el nombre de la fila → editá → Enter (o blur). Lo mismo con el
campo Fade. Cambios se persisten al instante.

---

## El Programmer (touched-tracking)

### Qué es

Un set en memoria de "fixtures que el operador tocó desde el último
Clear/Record". Liviano: solo guarda los IDs, no duplica los valores.

### Cuándo se llena

- Movés un slider en Stage → fixture queda marcado.
- Click en un botón de range (color wheel, strobe modes) → marcado.
- Cualquier write vía `setFixtureChannel` desde la UI.

### Cuándo se limpia

- Click **Clear** en la programmer bar.
- (Por ahora) **NO** se limpia automáticamente al hacer Record. Esto es a
  propósito: te permite grabar varias variaciones del mismo set de
  fixtures (ej. "intro fría" → record → ajusto color → "intro tibia" →
  record → no toco más nada → clear). Si querés el comportamiento Avolites
  donde Record también limpia, decime y lo cambio en una iteración futura.
- `New Show` y `Open Show` lo resetean (el programmer es per-sesión).

### Cuándo NO afecta

- Recallar una escena no marca nada como touched (es el motor el que
  está moviendo los faders, no vos).
- Direct DMX (en Config) bypassea todo y no toca el programmer.
- Chasers / Movements / Blackout / Blind no tocan el programmer — son
  capas independientes que componen sobre la base.

---

## Launchpad MK2

Si conectás un Launchpad (auto-detect on launch), la **fila 3** (notes
31-38, tercera desde abajo en la grilla 8×8) controla las primeras 8
escenas:

| Pad | Función |
|-----|---------|
| Press en pad iluminado | Recall esa escena con su fade configurado |
| Press en pad apagado | No-op (slot vacío, no hay escena en esa posición) |

### Feedback LED

- **Apagado**: no hay escena en esa posición.
- **Solid dim** (color de la paleta): la escena existe, no está activa.
- **Flash dim/bright**: la escena está activa (parpadea entre el color
  tenue y el brillante mientras es la última recalleada).

Cada pad tiene su propio color (white / light blue / blue / sea-green /
pale teal / pastel rose / pastel violet / mint), elegidos del lado frío
de la rueda para distinguirse de chasers (row 1, calidos) y movements
(row 2, pinks/purples).

### Layout completo del MK2 mientras hay show corriendo

```
   ┌─────────────────────────────────────────────┐
   │ ↑   ↓   ←   →   S   U1  U2  M  ← top row    │   chaser slot mirror
   ├─────────────────────────────────────────────┤
8  │                                          ●  │   scene buttons col
7  │                                          ●  │     (no usados aún)
6  │                                          ●  │
5  │                                          ●  │
4  │ □   □   □   □   □   □   □   □            ●  │
3  │ ●   ●   ●   ●   ●   ●   ●   ●            ●  │   ← scenes (row 3)
2  │ ●   ●   ●   ●   ●   ●   ●   ●            ●  │   ← movements (row 2)
1  │ ●   ●   ●   ●   ●   ●   ●   ●         ◯  ◯ │   ← chasers (row 1)
   │                                       BLD BO │   blind / blackout
   └─────────────────────────────────────────────┘
```

Las flechas ↑/↓ del top row además ajustan el BPM del chaser activo
(±1). El blackout (note 19) es toggle, el blind (note 29) es momentary.

---

## Casos de uso

### Show con cuelist manual

1. Armá tu primer cue (intro). Record con fade `2000ms`.
2. Para el cue 2 (verse 1): editás solo lo que cambia. Record (touched
   only) con fade `800ms`.
3. Para el cue 3 (chorus drop): edits → Record con fade `0` para snap.
4. Repetí. Resultado: lista de escenas en orden, las disparás a mano
   (Launchpad row 3) o con click ▶ GO.

### Cambiar el "look base" del show

1. Recallá la escena base.
2. Reajustá los fixtures que no te convencen.
3. Click ⟳ Update (la versión completa) en la fila → actualizada.
4. Si querés solo un fixture específico, ⟳ Touched después de tocarlo.

### Combinar escena + chaser ambiental

Las escenas escriben a la base. Los chasers / movements son una capa de
overlay que pinta encima de la base. Entonces:

1. Recallá la escena (define color/intensidad/pan-tilt base).
2. Activá un chaser sobre algunos de esos fixtures (ej. blink rojo).
3. El chaser pinta sus slots, los demás fixtures siguen en el estado de
   la escena.

Si grabás durante un chase corriendo, el snapshot queda con el frame
congelado del chase en ese instante. Eso a veces es lo que querés ("dejá
fija esa pinta del strobo"), a veces no — apagá el chaser antes de
record si querés solo la base.

### Update incremental para tweaks en sound check

Recallá la escena con la que vas a empezar la noche. Mientras suena la
banda en prueba:

1. Tocás 2 movers para reapuntar.
2. ⟳ Touched en la fila → solo esos 2 quedan actualizados.
3. Tocás el dimmer general de los pares.
4. ⟳ Touched de nuevo.
5. Clear cuando estás conforme.

El resto del show queda intacto.

---

## Limitaciones (Phase 4 MVP + iteración 2)

Lo que **funciona hoy**:

- ✅ Grabar / recall con fade.
- ✅ Touched-tracking + record selectivo + update selectivo.
- ✅ Launchpad ejecuta scenes con feedback LED.
- ✅ Persistencia: scenes viajan en el `.json` del show.
- ✅ Encadenar recalls sin snap-back.

Lo que **todavía no**:

- ❌ **Cuelist con Go/Back**: no hay sequencing automático. Cada escena
  se recalleá individualmente. (Phase 5 del plan original.)
- ❌ **Follow times**: la idea "cue siguiente arranca solo en N
  segundos" — Phase 5.
- ❌ **Submasters**: faders 0-100% que escalan la intensidad de un grupo
  — Phase 5.
- ❌ **LTP overlay programmer**: hoy un manual `set_channel` durante un
  fade de recall **es overrideado** por el siguiente tick del fade. El
  modelo Avolites pleno (manual writes "ganan" sobre cualquier
  playback) requiere reroutear `set_channel` a una capa overlay separada
  — agendado pero no en este sprint.
- ❌ **Atajos de teclado** (Space = Go, etc.) — Phase 5.
- ❌ **Reordenar escenas** drag-and-drop — pendiente.
- ❌ **Scenes 9+ en el Launchpad**: solo las primeras 8 están mapeadas.
  La 9+ se recalleán solo desde la UI por ahora.

---

## Troubleshooting

**Pongo GO y no pasa nada visible.**
La escena pudo haber grabado todos los canales en `0`. Recheck la
columna "fixtures · canales" — si el conteo es razonable, el problema
puede ser que el master del rig esté abajo, blackout activo, o blind
cambiando los valores. Probá apagar globales primero.

**El pad del Launchpad no se ilumina al GO.**
Verificá que el Launchpad esté conectado (Config → MIDI → debe decir
"surface activo"). Si recién conectaste, el feedback se activa al
próximo tick del feedback thread (~150ms).

**Re-grabé una escena y se perdieron fixtures que tenía.**
⟳ Update y ⟳ Touched **nunca agregan ni sacan fixtures** de la escena.
Si una escena tiene fixtures que ya borraste del patch (re-patch sin el
mover X), `update_scene_from_state` se saltea esos sin error. Si querés
agregar un fixture nuevo a una escena existente, por ahora la única vía
es: borrar la escena y re-grabarla. (Mejora futura: pickeo de fixtures
en la fila de update.)

**Activé Touched only y no graba nada.**
La barra programmer arriba dice "PROG · 0 fixtures tocados". Necesitás
mover algún slider en Stage primero. Si ya lo hiciste pero no aparece,
revisá que estés modificando vía la UI (no Direct DMX, que bypassea).

**El recall me pisó algo que estaba editando.**
Conocido: por ahora el motor de scene playback escribe a la base
mientras el fade corre, sobreescribiendo manual edits. Workaround:
esperá a que el fade termine, o recall y después editás. Cuando llegue
el LTP overlay programmer en una iteración futura, esto se arregla
solo.

---

## Próximas iteraciones (post-MVP)

Si tenés ganas/necesidad, cualquiera de estas se puede priorizar:

1. **Reorder de escenas** (drag handles) y **agrupar en folders**.
2. **Auto-name** desde el contenido (ej. "Red wash · 4 fixtures").
3. **Scene preview**: hover sobre la fila → mini-thumbnail de cómo se ve.
4. **Ramp curves**: linear / ease in / ease out en el fade (hoy es
   linear).
5. **Cuelist Phase 5**: secuencia con Go/Back/Pause y follow times.
6. **Programmer pleno**: capa LTP que sobreescribe scene playback con
   manual writes, Clear que devuelve el control al cuelist.
