# orcadeck-builtin

Acciones Orca integradas en el demonio, sin proceso de plugin ni WebSocket. Es el port a Rust del
plugin Node `com.miguelpicado.orcadock.sdPlugin` (`core.js` + `plugin.js`).

El demonio crea un `Registry`, le pasa las teclas de la escena y las pulsaciones, y rasteriza el SVG
que devuelve. Toda la lógica pura está separada del I/O para poder probarla sin hardware.

## Ficheros

| Fichero | Responsabilidad |
|---|---|
| `src/core.rs` | lógica pura: agentes principales, asignación a teclas, SVG de agente/reloj/resumen/cuota |
| `src/orca.rs` | trait `Orca` + `PipeOrca` (tubería con nombre de Orca), `raise` de la ventana, hora local |
| `src/actions.rs` | `Deck`, trait `BuiltinAction`, las cuatro acciones y el `Registry` |
| `src/raster.rs` | SVG → RGBA a 85×85 (teclas) o 80×80 (laterales) con `resvg` |
| `tests/port.rs` | paridad con `test.js`, `Registry` con Orca simulado y rasterizado |

## Acciones

| UUID | Nombre | Qué hace |
|---|---|---|
| `com.miguelpicado.orcadock.agent` | agente | un agente principal por tecla: repo, color, estado, tiempo en el estado y ⚙N workers vivos; pulsar enfoca su panel |
| `com.miguelpicado.orcadock.quota` | cuota | una cuenta por tecla desde `quota-status.json`: 5 h, semanal y mensual; pulsar escribe `.wake` |
| `com.miguelpicado.orcadock.clock` | reloj | HH:MM y fecha en una celda lateral |
| `com.miguelpicado.orcadock.summary` | resumen | agentes vivos en working / waiting / done |

Los UUID coinciden con los del plugin Node, así que las escenas existentes siguen valiendo sin
tocarlas.

## Uso

```rust
use orcadeck_builtin::{now_ms, Deck, Key, PipeOrca, Registry};

let mut registry = Registry::new(Deck::new(Box::new(PipeOrca::from_env()), Deck::default_quota_dir()));
registry.appear(Key {
    uuid: "com.miguelpicado.orcadock.agent".into(),
    context: "2,1".into(),
    device: "AKP153E".into(),
    row: 1,
    col: 2,
});

for (context, svg) in registry.tick(now_ms()) {
    // rasterizar y pintar
}
let feedback = registry.key_down("2,1", now_ms()); // Feedback::None | Ok | Alert
```

El demonio llama a `tick` cada ~400 ms y pinta solo las entradas devueltas (el `Registry` deduplica
el SVG sin cambios). `appear`/`disappear` se llaman al colocar una escena y `key_down` al pulsar; el
firmware entrega la pulsación al soltar, así que `keyDown` es el único evento.

## Datos

`Deck` comparte un poll de Orca y una lectura de cuota entre todas las teclas:

| Dato | Cadencia | Fuente |
|---|---|---|
| agentes | 1 s | `worktree.ps`, `terminal.list`, `orchestration.workerList`, `repo.list`, `orchestration.runList` |
| repos | 30 s | `repo.list` |
| workers y runs | 10 s | `orchestration.workerList` / `orchestration.runList` (~450 ms) |
| cuota | 30 s | `<QUOTA_DIR>/quota-status.json`; si es viejo (> 10 min), la tecla se atenúa |
| refresco forzado | 3 s hasta 45 s | escribe `<QUOTA_DIR>/.wake`; `orca-dock-rs` lo recoge |

`PipeOrca` lee `%APPDATA%\orca\orca-runtime.json`, habla JSON por líneas por la tubería con nombre,
usa timeout de 5 s y reintenta una vez releyendo el fichero (Orca puede haber reiniciado con otra
tubería y otro token). `raise` trae la ventana de Orca al frente.

`Deck::default_quota_dir()` usa `QUOTA_DIR` o, si no, `%APPDATA%\OrcaDeck\quota` (`<temp>\OrcaDeck\quota` sin `APPDATA`).

## Tests

```powershell
cd daemon
cargo test --release -p orcadeck-builtin
```

`tests/port.rs` replica las aserciones de `test.js` del plugin Node y añade pruebas de `Registry` con
un `Orca` simulado y de rasterizado. No abre el HID ni habla con Orca real.

## Límites

- Sin hardware: el SVG se genera y se rasteriza, pero el pintado lo hace `app.rs`.
- La cuota es de solo lectura; el productor es `orca-dock-rs`.
- `workerList` bloquea el hilo `builtin` ~450 ms cada 10 s; por eso el registro vive en su propio
  hilo y no en el del dispositivo.
- Sin pulsación larga: el firmware solo informa al soltar.
