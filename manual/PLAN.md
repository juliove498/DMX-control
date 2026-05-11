# Plan del manual — DMX Control

Hoja de ruta del manual para operadores. Vive en `manual/` para que se
actualice junto al pipeline de capturas y los archivos MDX.

## Estado actual

| # | Capítulo | Estado | Capturas |
|---|----------|--------|----------|
| 1 | Bienvenida | ✓ landing | 0 |
| 4 | El Stage | ✓ production-ready | 2 |
| 5 | Escenas | ✓ production-ready | 3 |
| 7 | Patch | ✓ production-ready | 2 |
| 11 | Chasers | ✓ production-ready | 2 |
| 12 | Movements | ✓ production-ready | 2 |
| 13 | Stream Deck | ✓ production-ready | 1 |

## Convenciones de autoría

- **Marcadores cortos en la imagen + prosa detallada en el MDX.** Si un
  callout necesita más de 3-4 palabras, conviene reemplazarlo por un
  `rect`/`circle` numerado y poner el texto explicativo en una lista
  numerada del MDX. La numeración de la lista debe coincidir 1-a-1 con
  la cantidad de marcadores en la imagen.
- **Coordenadas explícitas** sólo cuando no hay un elemento DOM en el
  punto que querés señalar — el resto, selectores con `data-doc=`.
- **Sembrar `data-doc` antes de escribir capturas.** No anclar a
  selectores frágiles (`.btn:nth-child(3)`) — quiebran en cuanto el
  layout cambia.
- **Idioma**: español (es). El sitio está configurado para multi-locale
  pero por ahora arrancamos monolingüe.

---

## Estructura propuesta

### Parte I — Primeros pasos
> Para alguien que abrió la app por primera vez.

1. **Bienvenida** (existe, mejorar)
   - Qué resuelve el software, a quién está dirigido, cómo está
     organizado el manual.
2. **Conceptos fundamentales** (~0 capturas, glossary)
   - Fixture, patch, escena, chaser, movement, blackout, blind,
     programmer, BPM master, fade, hold, FX state.
   - Diagrama conceptual de cómo se mezclan escenas + chasers + movement.
3. **Tu primer show** (~6 capturas)
   - Walkthrough end-to-end: nuevo show → agregar 2 fixtures desde la
     biblioteca → patchearlos → recordar un color → guardar como escena
     → recallarla.

### Parte II — Operación en vivo
> Lo que usás durante el show.

4. **El Stage** ✓
5. **Escenas** ✓
6. **Controles globales** (~4 capturas)
   - BPM master + tap (cuándo usarlo, cómo se propaga a chasers/movement).
   - Blackout: toggle, fade configurable, qué fixtures afecta.
   - Blind: hold-to-activate, preview en cabina sin tocar la salida.
   - Popouts a segundo monitor.

### Parte III — Programación
> Lo que hacés antes del show, de cero al rig listo.

7. **Patch** ✓ (2 capturas — vista general + estado de conflicto).
   Cubre el formulario de Add (fixture / mode / qty / U / addr /
   label), el badge de status, la tabla con edición in-place y el
   panel de issues que se enciende cuando dos fixtures se pisan.
   Vive en Config → Patch (no es una pestaña top-level).
8. **Biblioteca de fixtures** (~4 capturas)
   - Qué viene incluido, formato de definición, agregar fixture custom,
     editar imágenes y rangos de canal.
9. **Salidas DMX** (~5 capturas)
   - sACN multicast, Open DMX (FTDI), Enttec, bindings universo→salida.
   - Drivers de FTDI en macOS (link a README, no duplicar).
10. **Edición de fixture** (~4 capturas)
    - Encoders por canal, color picker 2D, pan/tilt pad, locate flash,
      multi-selección.

### Parte IV — Generadores ambientales
> El telón de fondo del show.

11. **Chasers** ✓ (2 capturas — vista general + anatomía expandida).
    Cubre prioridad vs escenas, ejemplos incluidos, los tres tabs del
    editor (Pattern & Color / Timing & Fade / Slots) y la mezcla con
    escenas vía SceneFxState.
12. **Movements** ✓ (2 capturas — vista general + close-up de
    parámetros). Cubre la lista, el editor con sus 5 fieldsets,
    el preview SVG interactivo y el filtrado por pan/tilt.

### Parte V — Control externo
> Para el que sale del mouse y empieza a usar las manos.

13. **Stream Deck** ✓ (1 captura — config view con device conectado).
    Cubre el flujo Refrescar/Conectar/Desconectar y el layout fijo
    (chasers/movements/scenes en filas dedicadas) que es lo
    realmente implementado hoy. No hay assignment por tecla aún.
14. **MIDI** (~4 capturas)
    - Asignar notas a escenas/chasers/master, soporte específico para
      Launchpad MK2 (color feedback).
15. **Remote (móvil)** (~5 capturas)
    - Bridge LAN, pairing por QR/PIN, qué controles aparecen en el
      celular, revocación de devices.

### Parte VI — Avanzado
> Para el que ya domina lo básico.

16. **Generación de escenas con IA** (~4 capturas)
    - Config de provider (OpenAI / Anthropic / local), prompt para
      crear escena nueva, iterar sobre escena existente.
17. **Preview 3D** (~3 capturas)
    - Visualizador del rig, atajos de cámara, cuándo conviene tenerlo
      abierto.
18. **Sync entre máquinas** (~4 capturas)
    - Sync de config vía Gist privado, qué se incluye/excluye, PAT en
      keychain.

### Parte VII — Apéndice
> Información de referencia, no de tutorial.

19. **Resolución de problemas**
    - FTDI en macOS (link a README), logs (`~/Library/Logs/dmx-control/`),
      ubicación de archivos del show, recuperación desde backup.
20. **Atajos de teclado y referencia rápida**
    - Chuleta imprimible.

---

## Estimación

- ~20 capítulos
- ~70-90 capturas en total
- Tiempo por capítulo (con la mecánica actual): 30-60 min entre
  sembrar `data-doc`, mockear el estado, escribir 3-5 capturas y
  redactar el MDX.

## Próximos candidatos para escribir

En orden de impacto sugerido (subjetivo, ajustable):

1. **Tu primer show** — onboarding crítico, ahora que Patch ya está
   escrito puede referenciarlo.
2. **Conceptos fundamentales** — sin capturas, glossary; barato.
3. **Controles globales** — capturas chicas sobre el header.

Capítulos baratos para hacer cuando quede ratos sueltos: **Conceptos
fundamentales** (sin capturas), **Controles globales** (capturas chicas
sobre el header), **Atajos de teclado** (cero capturas).
