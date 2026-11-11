# Desarrollo

Guía para compilar, probar y tocar OrcaDeck sin romper el objetivo del proyecto: un binario Rust
ligero, sin runtime async, con 0 % de CPU en reposo y UI web solo bajo demanda.

- Uso: [USAGE.md](USAGE.md). Arquitectura: [ARCHITECTURE.md](../design/ARCHITECTURE.md).
- Fallos: [TROUBLESHOOTING.md](TROUBLESHOOTING.md). Estado: [HANDOFF.md](../project/HANDOFF.md),
  [ROADMAP.md](../project/ROADMAP.md).

## 1. Requisitos

- Windows 10/11 x64.
- Rust `stable-x86_64-pc-windows-gnullvm` (fijado en [`daemon/rust-toolchain.toml`](../../daemon/rust-toolchain.toml);
  **no lo edites**).
- En esta máquina no hay linker MSVC ni admin: se usa **llvm-mingw portable** en
  `C:\Tools\llvm-mingw-20261104-ucrt-x86_64\bin`, que debe estar en `PATH`.
- Para ejecutar el `.exe`: `libunwind.dll` del mismo llvm-mingw en `PATH` o junto al binario.

## 2. Entorno de compilación (PowerShell, desde `daemon/`)

```powershell
$env:PATH="C:\Tools\llvm-mingw-20261104-ucrt-x86_64\bin;$env:USERPROFILE\.cargo\bin;$env:PATH"
$env:RUSTUP_HOME="%USERPROFILE%\.rustup"
$env:CARGO_HOME="%USERPROFILE%\.cargo"
$env:RUSTUP_TOOLCHAIN="gnullvm-local"   # toolchain local; rust-toolchain.toml no se toca
cd <repo>\daemon

cargo build --release
cargo test --release --workspace
```

- El binario queda en `daemon\target\release\orcadeck.exe`; el instalador de plugins, en
  `daemon\target\release\orcadeck-plugman.exe`.
- `RUSTUP_TOOLCHAIN=gnullvm-local` es el apaño de esta máquina porque rustup ya no instala el canal
  `gnullvm`. En otra máquina, instala el toolchain que fija `rust-toolchain.toml`.
- Si trabajas en paralelo con otros agentes, usa un target propio para no bloquear el `target`:
  ```powershell
  $env:CARGO_TARGET_DIR="target-$env:USERNAME"
  ```
- El perfil release ya está ajustado en [`daemon/Cargo.toml`](../../daemon/Cargo.toml): `opt-level = "s"`,
  LTO, `codegen-units = 1`, `panic = "abort"`, `strip`.

## 3. Gate antes de dar algo por terminado

```powershell
cd daemon
cargo test --release --workspace
```

Es el único gate del repositorio. Debe compilar el workspace completo (binario + `host`, `plugman`,
`builtin`, `web`) y pasar la suite. El CI (`.github/workflows/ci.yml`) ejecuta lo mismo en cada push,
más el smoke del binario release (`tools/smoke/smoke.ps1`) y la suite E2E de `tools/e2e`.

Medidas que acompañan a un cambio de rendimiento (anotarlas, no estimarlas):

```powershell
(Get-Item .\target\release\orcadeck.exe).Length          # tamaño
Get-Process orcadeck | Select-Object CPU, WorkingSet     # CPU acumulada y RSS con el dispositivo
$env:ORCADECK_TRACE=1; .\target\release\orcadeck.exe run <perfil>   # entradas y duración de lotes
```

## 4. Estructura de tests

En el árbol hay **89 tests verdes** (contados el 2026-11-10 con el gate; 87 `#[test]` y 2
doc-tests). La mayoría son unitarios y no necesitan hardware; las de integración usan sockets de
loopback y carpetas temporales.

| Dónde | Qué cubre |
|---|---|
| `daemon/src/proto.rs` | mapa de teclas, comandos CRT, troceado de imágenes, parseo de entrada, `N` del descriptor |
| `daemon/src/render.rs` | JPEG ≤ 9000 B, tamaño 85/80, rotación |
| `daemon/src/profile.rs` | lectura del manifiesto real, escritura de solo `Settings` sin perder el resto |
| `daemon/src/scenes.rs` | siguiente/anterior con vuelta, id explícito con/sin `.sdProfile`, id desconocido, escena actual fuera de orden |
| `daemon/src/app.rs` | debounce de ajustes, no reescritura si no cambia, retirada de contextos, globales, URLs, composición de título/feedback |
| `daemon/src/edit.rs` | resolución de rutas por defecto y con overrides |
| `daemon/crates/builtin/tests/port.rs` | paridad con `test.js` del plugin Node, `Registry` con Orca simulado, rasterizado |
| `daemon/crates/host/tests/fake_plugin.rs` | WebSocket real de loopback con un plugin falso: registro, `willAppear`, teclas, ajustes, PI, origen, reinicio/stop |
| `daemon/crates/host/tests/real_plugin.rs` | plugin Node real; solo corre con `ORCADOCK_PLUGIN` apuntando a la carpeta del plugin y `node` disponible (si no, se omite) |
| `daemon/crates/plugman/tests/plugman.rs` | zips de fixture, manifests cifrados/planos, overlay, `force`/backup, límites, traversal |
| `daemon/crates/web/tests/api.rs` | HTTP real en loopback: seguridad (token/Host/Origin), escenas, celdas, plugins, imágenes |
| `daemon/crates/web/tests/real_folders.rs` | lectura de carpetas reales de Stream Dock; se **omite** si no existen |
| `tools/e2e/` (Playwright) | E2E del editor con el `orcadeck.exe` release: arranque y token, edición de una celda, instalación/quitado de plugins, accesibilidad y errores de consola; sin hardware |
| `tools/smoke/smoke.ps1` | smoke del binario release: `help`, `render` offline y `edit` standalone, con rutas temporales |

Los tests que dependen de datos reales no fallan si faltan: imprimen `skipped` y pasan. No leen ni
escriben el dispositivo.

## 5. Probar sin dispositivo

El HID solo hace falta para `probe`, `scene`, `bright`, `watch`, `poll` y `run`. Sin teclado:

1. **Unitarios**: `cargo test --release --workspace`.
2. **Render offline** sobre una copia del perfil:
   ```powershell
   .\target\release\orcadeck.exe render <perfil.sdProfile> .\out
   ```
   Abre los `keyNN.jpg` y comprueba composición, rotación y tamaños. No abre el HID.
3. **Editor** con copias, nunca contra los perfiles reales:
   ```powershell
   $env:ORCADECK_PROFILES="C:\temp\perfiles-copia"
   $env:ORCADECK_PLUGINS="C:\temp\plugins-copia"
   $env:ORCADECK_DATA="C:\temp\datos-copia"
   .\target\release\orcadeck.exe edit --no-open
   ```
4. **Plugman**:
   ```powershell
   .\target\release\orcadeck-plugman.exe inspect <paquete-o-carpeta>
   .\target\release\orcadeck-plugman.exe install <paquete> --plugins C:\temp\plugins --data C:\temp\datos --force
   ```
5. **`run` sin dispositivo**: arranca el host y el hilo `builtin`, pero `session` falla y reintenta
   cada 2 s (`device: …; retrying in 2 s`). Sirve para ver que el host enlaza el 18618 y que los
   plugins de la escena se lanzan; no para validar pintado ni pulsaciones.

La prueba real (pintado, pulsaciones, escenas, cuotas) es manual y requiere el AKP153E con Stream
Dock parado. No la automatices ni la metas en un test.

## 6. Reglas de código

- **Un solo escritor del HID**: el bucle de `session`. Cualquier pintado nuevo pasa por
  `Runtime::send`/`paint` y el mismo lote. No abras el dispositivo desde otro sitio.
- **Sin polling nuevo**. Los hilos duermen en lecturas bloqueantes o `recv_timeout` con una fecha
  límite. Los temporizadores (feedback, ajustes) van en `Runtime::tick`, que ya se despierta cada
  30 ms para la entrada.
- **Sin runtime async ni hilos por evento**. El host usa tungstenite síncrono y un hilo por socket;
  las llamadas a Orca usan un hilo efímero por el timeout de las tuberías de `std`.
- **Sin dependencias nuevas** salvo necesidad demostrada y medida. El objetivo es un binario de
  pocos MB y arranque inmediato.
- **Nada de UI residente**. `edit` es un proceso aparte que se apaga; `run` no sirve HTTP.
- **No perder datos**: escribe con `.tmp` + rename, conserva el fichero anterior cuando toca, lee sin
  BOM y escribe con el formato de Qt (claves ordenadas, 4 espacios).
- **Valida en los límites de confianza**: ids con `safe_id`, rutas canonicalizadas dentro de su
  carpeta, URL solo `http(s)`, `Origin`/token, límites de zip.
- **Cada lógica nueva no trivial deja un test** pequeño; los cambios triviales no.
- Si aplazas algo con un límite conocido, deja un comentario `ponytail: <límite>, <cuándo>` o
  `shortcut: <límite>, <cuándo>`, como el resto del repo.

## 7. Dónde tocar

| Quiero… | Fichero(s) |
|---|---|
| añadir un subcomando | [`daemon/src/main.rs`](../../daemon/src/main.rs) (`match a(0)`), módulo nuevo si crece |
| añadir una acción integrada | [`crates/builtin/src/actions.rs`](../../daemon/crates/builtin/src/actions.rs): `BuiltinAction`, `Registry::new`, y el render en `core.rs` |
| atender un comando de plugin | [`crates/host/src/host.rs`](../../daemon/crates/host/src/host.rs) (`fn command`) y `Output` |
| cambiar el pintado o los lotes | [`daemon/src/app.rs`](../../daemon/src/app.rs) (`Runtime`, `session`) |
| cambiar el formato de escena | [`daemon/src/profile.rs`](../../daemon/src/profile.rs) y `crates/web/src/store.rs`/`api.rs` |
| cambiar el editor | [`crates/web/web/`](../../daemon/crates/web/web) y `crates/web/src/api.rs` |
| instalar/reconstruir plugins | [`crates/plugman/src/lib.rs`](../../daemon/crates/plugman/src/lib.rs) |
| tocar el protocolo HID | [`daemon/src/proto.rs`](../../daemon/src/proto.rs), [`daemon/src/render.rs`](../../daemon/src/render.rs); contrasta `../research/hid/hid-protocol.md` |

Las escenas y los ajustes son datos del usuario: si un cambio implica migrarlos, escribe el código
para leer el formato viejo, no para reescribir sus ficheros.

## 8. Reglas duras de firmware

Estas reglas no se negocian y aplican también a tests y scripts:

1. **No flashear nada.**
2. **No lanzar `FirmwareUpgradeTool.exe` con 4 argumentos** (`768 4112 <bin> <icono>`): empieza a
   flashear de inmediato, sin confirmación.
3. **No enviar `CRT\0\0APP NEW`** ni ningún comando CRT de actualización.
4. `hidwatch` es el único instrumento permitido para ver el bootloader (`EEEF:2929`): solo enumera,
   no abre ni escribe.
5. Cualquier trabajo con el parche de `docs/research/firmware/` es análisis, no ejecución. El
   veredicto y el estado están en `research/firmware/patch-emulation.md`.

## 9. Higiene del repositorio

- No commitees los ficheros grandes de la raíz (zips, instalador, `.rar`, carpeta del driver); están
  en `.gitignore` y son del usuario.
- No muevas ni borres ficheros que no se hayan nombrado por ruta exacta.
- No toques `daemon/rust-toolchain.toml` ni los perfiles reales de Stream Dock.
- El CI (`.github/workflows/ci.yml`) cubre `cargo test --release --workspace`, el smoke del binario y
  la E2E; la verificación con hardware sigue siendo manual y con Stream Dock parado.

## 10. Releases

Las releases se publican desde el repo público (`github.com/miguelpicado/a-better-ajazz-controller`)
con [`.github/workflows/release.yml`](../../.github/workflows/release.yml), que compila `orcadeck.exe`
para Windows x64 al empujar un tag `v*` y adjunta
`orcadeck-<tag>-windows-x64.zip` (binario, `LICENSE-MIT`, `LICENSE-APACHE`, `THIRD-PARTY-LICENSES.md`
y `README.md`) más su `.sha256`, con notas generadas automáticamente.

Para cortar una release, desde `main` ya verificado y en el repo público:

```sh
git tag v0.1.0
git push origin v0.1.0
```

El workflow también se puede lanzar a mano (`workflow_dispatch`) con un `tag` y `publish`; si
`publish` no es `true` o el tag no empieza por `v`, solo deja el zip como artifact del run y no crea
release.

Verificar la descarga (PowerShell):

```powershell
Get-FileHash .\orcadeck-v0.1.0-windows-x64.zip -Algorithm SHA256
Get-Content .\orcadeck-v0.1.0-windows-x64.zip.sha256
```

Los dos hashes deben coincidir; el `.sha256` lleva el hash y el nombre del zip, con el formato que
espera `sha256sum -c`. El binario **no está firmado**: Windows SmartScreen puede avisar antes de
ejecutarlo. Las herramientas de firmware no forman parte de la release.

## 11. Publicar el espejo público

El repo privado se publica en `miguelpicado/a-better-ajazz-controller` con la historia limpia: sin
firmware del fabricante ni correos reales. No lo hagas a mano; usa el script (Node, sin
dependencias):

```powershell
node tools/publish-public.mjs --dry-run   # clona y filtra en %TEMP%; no empuja
node tools/publish-public.mjs             # publica main (push normal, sin --force)
```

- **Cuándo**: cuando `main` tenga algo que publicar y el árbol esté limpio. El script se niega si
  hay cambios sin commitear, porque solo se publica la historia commiteada.
- **Qué elimina**: los cuatro `.bin` de `docs/research/firmware/` y cualquier `*.bin`, `*.exe` o
  `*.dll` de la historia (salvo lista blanca explícita), y reescribe los correos de autor y
  committer al noreply de GitHub (`<id>+<login>@users.noreply.github.com`).
- **Qué comprueba antes de empujar**: que no queden binarios ni correos ajenos, que no aparezca
  ninguna cadena de `tools/publish-denylist.txt` (opcional, local y gitignorado: una por línea, `#`
  para comentarios) y que sigan existiendo `LICENSE-MIT`, `LICENSE-APACHE` y `README.md`. Si algo
  falla, sale con código 1 y no empuja nada.
- **Requisitos**: `git filter-repo` en el `PATH` y `gh auth status` correcto.
- **Nunca** ejecutes `git filter-repo` en este repo: reescribe todo el historial. El script clona
  con `--no-local` en un temporal, filtra solo ahí y borra el temporal al terminar
  (`--keep-tmp` lo conserva).
- El push es `main:main` sin `--force`. Si lo rechazan por non-fast-forward, para y revisa qué hay
  en el espejo; no fuerces.
