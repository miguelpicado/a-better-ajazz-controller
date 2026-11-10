# Auditoría Integral de Salud del Repositorio: orca-stream-dock (orcadeck)

> **Nota (2026-11-10, posterior):** las rutas `file:///...repo-audit` y `docs/*.md` de este informe
> son del worktree y del árbol anterior a la reorganización de `docs/`; se conservan como evidencia.
> Varias acciones se han hecho después (documentación alineada, legacy archivado); lo que sigue
> pendiente está en [`STATUS.md`](STATUS.md) y [`ROADMAP.md`](ROADMAP.md).

- **Fecha:** 2026-11-10
- **Repositorio:** `C:\Users\migue\orca\workspaces\orca-stream-dock\repo-audit` (worktree de `C:\Users\migue\Github\orca-stream-dock`)
- **Rama:** `miguelpicado/repo-audit`
- **Herramienta / Entorno:** Antigravity / Gemini 3.8 Flash, Rust `gnullvm-local` (1.99.0) + `llvm-mingw-20261104-ucrt-x86_64`
- **Objetivo rector del proyecto:** Sustituto en Rust extremadamente ligero y optimizado de la app oficial "Stream Dock AJAZZ", 0 % CPU en reposo, sin dependencias superfluas, un solo binario, UI web bajo demanda (`orcadeck edit`) nunca residente.

---

## 1. Árbol Rastreado vs Ignorado vs Sin Seguimiento

### 1.1 Evidencia y comandos ejecutados
- Árbol rastreado: `git ls-files` (77 ficheros registrados).
- Estado de ficheros ignorados y sin seguimiento: `git status --ignored` tanto en el worktree como en el repositorio principal (`C:\Users\migue\Github\orca-stream-dock`).
- Medición de tamaño en disco por directorio:
  - Comando PowerShell:
    ```powershell
    Get-ChildItem -Force C:\Users\migue\Github\orca-stream-dock | ForEach-Object {
        $item = $_
        if ($item.PSIsContainer) {
            $size = (Get-ChildItem -Recurse -Force $item.FullName -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum).Sum
            [PSCustomObject]@{ Name = $item.Name; Type = "Dir"; SizeMB = [math]::Round($size / 1MB, 2) }
        } else {
            [PSCustomObject]@{ Name = $item.Name; Type = "File"; SizeMB = [math]::Round($item.Length / 1MB, 2) }
        }
    } | Sort-Object -Descending SizeMB
    ```

### 1.2 Volumetría de los elementos mayores

| Elemento | Tipo | Tamaño aprox. | Estado Git | Justificación / Contenido |
|---|---|---|---|---|
| `daemon/` | Directorio | 1 236,74 MB | Rastreado (código) / Ignorado (`target/`) | Código Rust del daemon y sus crates. La mayor parte del espacio corresponde a `daemon/target/` (artefactos de compilación gnullvm). |
| `Stream-Dock-AJAZZ-Installer_Windows_global.exe` | Archivo | 534,95 MB | Ignorado (`.gitignore:14`) | Instalador original oficial de AJAZZ (build 8e91648f…). Fichero del usuario para análisis forense; no commitear. |
| `docs/` | Directorio | 311,23 MB | Rastreado (docs) / Ignorado (`work/extract-*`) | Documentación y research. Los ~310 MB provienen de volcados extraídos de las apps oficiales en `docs/research/firmware/work/extract-2.10.185/` y `current-3.10.191/`. |
| `AJAZZ_AKP153E&AKP815&AKP03E_Screen Driver (…) _V2.10.185` | Directorio | 263,55 MB | Ignorado (`.gitignore:13`) | Instalador descomprimido por el usuario con herramientas externas. |
| `AJAZZ_AKP153E&AKP815&AKP03E_Screen Driver (…) _V2.10.185.rar` | Archivo | 251,60 MB | Ignorado (`.gitignore:13`) | Fichero comprimido RAR5 aportado por el usuario. |
| `.git/` | Directorio | 1,59 MB | Metadatos Git | Repositorio git principal y directorio de worktrees. |
| `AJAZZ_ak820_pro_2.4G_upgrade.zip` | Archivo | 0,82 MB (863 KB) | Ignorado (`.gitignore:12`) | Fichero de actualización de referencia de otro dispositivo (teclado AK820 Pro). |
| `com.miguelpicado.orcadock.sdPlugin` | Directorio | 0,18 MB (189 KB) | Rastreado / Ignorado (`node_modules/`) | Prototipo legacy en Node.js del plugin inicial. |
| `.playwright-mcp/` | Directorio | 0,13 MB (131 KB) | Ignorado (`.gitignore:9`) | Datos de sesión de navegación/pruebas de Playwright. |
| `tools/` | Directorio | 0,04 MB (41 KB) | Rastreado | Utilidades y scripts auxiliares. |
| `.opencode/` | Directorio | ~2,5 KB | **Sin seguimiento (pre-auditoría)** | Directorio interno de indexación y leases de workers concurrentes (`background-worker.*.lease`). |
| `daemon/target-audit/` | Directorio | ~150 MB | **Sin seguimiento (pre-auditoría)** | Directorio de compilación aislado generado por esta auditoría. |

### 1.3 Análisis de ficheros obsoletos, duplicados o huérfanos

1. **`deploy.ps1` (raíz):**
   - *Ruta:* [deploy.ps1](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/deploy.ps1#L1-L18)
   - *Propósito:* Script de PowerShell para compilar (`npm install`) e instalar el plugin `com.miguelpicado.orcadock.sdPlugin` dentro de la aplicación propietaria `Stream Dock AJAZZ.exe` en `%APPDATA%\HotSpot\StreamDock\plugins\`.
   - *Diagnóstico:* **Obsoleto / Huérfano**. El plugin en Node.js ya fue migrado íntegramente a Rust como acciones nativas en `daemon/crates/builtin`, y el daemon `orcadeck` sustituye a la app oficial. Además, contiene rutas locales absolutas cableadas (`C:\Program Files (x86)\Stream Dock AJAZZ Global\Stream Dock AJAZZ.exe`).
   - *Acción recomendada:* Retirar del árbol o trasladar a `tools/legacy/` cuando el coordinador lo decida.

2. **`com.miguelpicado.orcadock.sdPlugin` (raíz):**
   - *Ruta:* [com.miguelpicado.orcadock.sdPlugin](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/com.miguelpicado.orcadock.sdPlugin)
   - *Contenido:* `core.js`, `plugin.js`, `test.js`, `manifest.json`, `package.json`, `raise.ps1`, `imgs/`.
   - *Diagnóstico:* **Duplicado funcional / Legacy**. Fue el prototipo inicial en Node.js. En `docs/STATUS.md` (§1.16 y §1.23) y `docs/HANDOFF.md` (§1.64) se especifica que fue migrado a `daemon/crates/builtin` (`actions.rs`, `core.rs`, `orca.rs`, `raster.rs`), y su suite de pruebas (`test.js`) está portada a Rust en `daemon/crates/builtin/tests/port.rs`.
   - *Acción recomendada:* No borrar en este turno; planificar su archivado en una rama legacy o carpeta de referencia histórica `tools/legacy/`.

3. **`tools/streamdock`:**
   - *Ruta:* [tools/streamdock](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/tools/streamdock)
   - *Contenido:* `ARROW_LEFT.png`, `ARROW_RIGHT.png`, `arrows.ps1`, `insta360-manifest.json`, `profiles.mjs`.
   - *Diagnóstico:* **Herramientas de un solo uso (One-off) y artefactos estáticos**. El script `profiles.mjs` indica explícitamente en la línea 1: `// One-off: arrows on Orca scene, Orca Deck clock/summary on the side panel, Grabación scene. Run with Stream Dock stopped.`. Modificaba perfiles directamente en `%APPDATA%`. Actualmente, la manipulación de perfiles está integrada en `daemon/src/profile.rs`, `scenes.rs` y el editor web `orcadeck-web`. Las imágenes `ARROW_*.png` son generadas por `arrows.ps1`.
   - *Acción recomendada:* Conservar en `tools/streamdock/` como utilidades de mantenimiento y fixtures, o documentar su condición de herramienta puntual.

4. **`tools/plugin-install`:**
   - *Ruta:* [tools/plugin-install](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/tools/plugin-install)
   - *Contenido:* `install.mjs`, `install.test.mjs`, `README.md`.
   - *Diagnóstico:* **Prototipo de referencia sustituido**. Fue la implementación de referencia en Node para descifrado de estructura de plugins Elgato con `ELGATO` DRM y generación de overlays. Ha sido portado al 100% a Rust en `daemon/crates/plugman` (probado con 13 tests en `tests/plugman.rs`).
   - *Acción recomendada:* Conservar como especificación ejecutable de referencia o mover a `tools/legacy/plugin-install-node/`.

5. **`docs/design`:**
   - *Ruta:* [docs/design](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/docs/design)
   - *Contenido:* `architecture.md` (borrador del 2026-11-09) y `plugin-install-handoff.md`.
   - *Diagnóstico:* **Diseño fundacional histórico**. Valioso como trazabilidad, pero `architecture.md` contiene decisiones tempranas que fueron superadas (como la asunción de usar `axum` y `tokio-tungstenite`, o la espera de ACK en paquetes `BAT`, descartadas en la implementación final síncrona sin runtime async).
   - *Acción recomendada:* Conservar intactos, añadiendo una nota de vigencia que remita a `docs/HANDOFF.md`.

6. **`.opencode/`:**
   - *Ruta:* `.opencode/` en la raíz del repositorio.
   - *Diagnóstico:* Directorio generado por el entorno de agentes / indexadores OpenCode (`index/background-worker.*.lease`). Genera ruido en `git status`.
   - *Resolución aplicada:* **Añadido a `.gitignore`**.

7. **`target-*/`:**
   - Directorios de compilación temporal creados con `CARGO_TARGET_DIR=target-audit` (o carpetas de otros workers).
   - *Resolución aplicada:* **Añadido a `.gitignore`**.

---

## 2. Documentación: Afirmaciones Obsoletas y Contradicciones

### 2.1 Desalineación entre `docs/STATUS.md`, `docs/HANDOFF.md` y `docs/ROADMAP.md`

| Fichero y línea | Afirmación en el documento | Realidad comprobada en el código / estado actual | Gravedad |
|---|---|---|---|
| [STATUS.md:8-10](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/docs/STATUS.md#L8-L10) | *"cinco piezas independientes que aún no están integradas ni probadas con el dispositivo"* | Las cinco piezas **ya están integradas** en el workspace único `daemon/` con los subcomandos `orcadeck run` y `orcadeck edit`, probadas y con 74 tests verdes. Documentado en [HANDOFF.md:64-75](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/docs/HANDOFF.md#L64-L75). | Alta |
| [STATUS.md:26-27](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/docs/STATUS.md#L26-L27) | *"Cada crate de crates/ es independiente (su propio [workspace] vacío y rust-toolchain.toml); daemon/Cargo.toml no las referencia todavía."* | [daemon/Cargo.toml:7-8](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/daemon/Cargo.toml#L7-L8) ya declara `members = ["crates/host", "crates/plugman", "crates/builtin", "crates/web"]` con dependencias por path unificadas y un solo `Cargo.lock`. | Alta |
| [STATUS.md:12](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/docs/STATUS.md#L12) | *"Nada está commiteado: daemon/, docs/, tools/plugin-install/, .opencode/ y .playwright-mcp/ están sin seguimiento"* | En git, `daemon/`, `docs/` y `tools/plugin-install/` **sí están rastreados**. Solo `.opencode/` quedaba sin seguimiento. | Media |
| [STATUS.md:137-140](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/docs/STATUS.md#L137-L140) | *Pendiente #2: "Integración en el daemon: convertir daemon/ en workspace con las 4 crates..."* | Tarea completada. Ya es workspace y compila en conjunto. | Media |
| [hid-protocol.md:46-47](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/docs/research/hid-protocol.md#L46-L47) y [architecture.md:33-34](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/docs/design/architecture.md#L33-L34) | *"BAT cabecera de imagen ... seguida del JPEG en trozos de N bytes y espera de ACK..OK (<= 1 s)"* | Falso en el hardware real: capturas de Frida en vivo demostraron que el dispositivo **no envía ACK** de imágenes y que esperarlo provoca pérdida masiva de fotogramas ([HANDOFF.md:36, 72-76](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/docs/HANDOFF.md#L36)). | Alta |

### 2.2 Frases erróneas sobre UpDateTool y FirmwareUpgradeTool

Tal como señalaba `docs/HANDOFF.md:40-44`, los documentos de investigación preliminar asumieron inicialmente que `FirmwareUpgradeTool.exe` no servía para el AKP153E y que se requería una herramienta ausente llamada `UpDateTool11-11.exe`. El hallazgo posterior en `docs/research/updatetool-origin.md` refutó esto por completo: `FirmwareUpgradeTool.exe` **es** la herramienta oficial de flasheo cuando se le pasan 4 argumentos (`768 4112 <bin> <icono>`).

#### Localización exacta de afirmaciones erróneas en `docs/research/updatetool-analysis.md`:
1. [Línea 20](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/docs/research/updatetool-analysis.md#L20): `| …\Stream Dock AJAZZ Global\FirmwareUpgradeTool.exe | 1 809 384 B | — | Para el SoC Actions (familia d13x), no AKP153E |` — **Erróneo:** SÍ es para el AKP153E.
2. [Línea 21](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/docs/research/updatetool-analysis.md#L21): `| UpDateToolV2\UpDateTool11-11.exe | — | — | Ausente en todo el disco y en el instalador (ver §1) |` — **Desfasado:** UpDateTool no tiene relación con el flasheo de este dispositivo.
3. [Líneas 48-53](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/docs/research/updatetool-analysis.md#L48-L53): `"Conclusión §1: el EXE delega el flasheo en UpDateTool11-11.exe, que no se instala con este build ni viene como cadena en el instalador..."` — **Erróneo:** El flasheo real por HID lo ejecuta `FirmwareUpgradeTool.exe`.
4. [Líneas 79-82, 87](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/docs/research/updatetool-analysis.md#L79-L87): `"Quién envía ese disparo: Desconocido: ni EXE ni DLL lo construyen -> lo haría UpDateTool"` — **Erróneo:** Lo envía `FirmwareUpgradeTool.exe` al detectar que no está en modo bootloader.
5. [Línea 206](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/docs/research/updatetool-analysis.md#L206): `| ¿Herramienta de flasheo disponible? | No (UpDateTool ausente; no extraíble del instalador con lo presente) |` — **Erróneo:** La herramienta está instalada en disco.
6. [Líneas 214-215](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/docs/research/updatetool-analysis.md#L214-L215): `"1. Conseguir UpDateToolV2\UpDateTool11-11.exe ... y analizarlo en estático ... PARADA 1"` — **Erróneo:** Línea de investigación abandonada por irrelevante.

#### Localización exacta de afirmaciones erróneas en `docs/research/firmware-feasibility.md`:
1. [Líneas 12-16](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/docs/research/firmware-feasibility.md#L12-L16): `"En este equipo no existe ninguna herramienta que actualice el firmware del AKP153E... Mientras no se consiga el bootloader (volcado por SWD) o la herramienta oficial de actualización, no hay forma segura de flashear."` — **Erróneo.**
2. [Línea 21](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/docs/research/firmware-feasibility.md#L21): `| Carga en el dispositivo | Desconocida: no hay herramienta local y el bootloader no está en el .bin |` — **Erróneo:** `FirmwareUpgradeTool.exe` está presente localmente.
3. [Línea 33](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/docs/research/firmware-feasibility.md#L33): `| C:\Program Files (x86)\Stream Dock AJAZZ Global\FirmwareUpgradeTool.exe | 1 809 384 B | ... | Herramienta para otro chip, ver §3 |` — **Erróneo.**
4. [Líneas 104-105](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/docs/research/firmware-feasibility.md#L104-L105): `"FirmwareUpgradeTool.exe es para el SoC de Actions ... No habla HID ni CRT"` — **Erróneo:** Con 4 argumentos habla HID directo con el bootloader `EEEF:2929` y envía comandos `CRT\0\0APP NEW`.
5. [Líneas 107-110](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/docs/research/firmware-feasibility.md#L107-L110): `"Conclusión: con lo instalado no se puede reconstruir el protocolo HID con que el bootloader del AKP153E recibe la imagen..."` — **Erróneo:** El protocolo fue completamente reconstruido en `updatetool-origin.md`.
6. [Línea 172](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/docs/research/firmware-feasibility.md#L172): `"Pero el 'mismo mecanismo de la app' no existe para el AKP153E (§3.2)..."` — **Erróneo.**
7. [Líneas 180-182](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/docs/research/firmware-feasibility.md#L180-L182): `"1. Sin abrir nada: conseguir UpDateToolV2\UpDateTool11-11.exe ... PARADA 1"` — **Erróneo.**

---

## 3. Calidad Rust

### 3.1 `cargo fmt --check`
- **Toolchain `gnullvm-local`:** `cargo-fmt.exe` no se encuentra instalado en la distribución standalone en `C:\Tools\rust-gnullvm\bin`. Al invocar `cargo fmt --check` bajo `gnullvm-local`, cargo informa:
  `error: 'cargo-fmt.exe' is not installed for the custom toolchain 'gnullvm-local'.`
- **Comprobación con toolchain estándar (`stable-x86_64-pc-windows-msvc`):**
  Al ejecutar `cargo +stable fmt --check`, se constatan diferencias de estilo en **28 ficheros** del workspace (ningún fichero Rust ha sido formateado con `rustfmt` estándar):
  - `daemon/src/main.rs`, `daemon/src/app.rs`, `daemon/src/edit.rs`, `daemon/src/profile.rs`, `daemon/src/proto.rs`, `daemon/src/render.rs`, `daemon/src/scenes.rs`.
  - `daemon/crates/builtin/src/actions.rs`, `core.rs`, `orca.rs`, `raster.rs`, y su test `tests/port.rs`.
  - `daemon/crates/host/src/host.rs`, `launch.rs`, `manifest.rs`, y tests `fake_plugin.rs`, `real_plugin.rs`.
  - `daemon/crates/plugman/src/lib.rs`, `main.rs`, y `tests/plugman.rs`.
  - `daemon/crates/web/src/api.rs`, `http.rs`, `lib.rs`, `sqlite.rs`, `store.rs`, `examples/serve.rs`, y tests `tests/api.rs`, `real_folders.rs`.

### 3.2 `cargo clippy --release --workspace --all-targets`
- **Toolchain `gnullvm-local`:** `cargo-clippy.exe` no está instalado en `C:\Tools\rust-gnullvm\bin`.
  `error: 'cargo-clippy.exe' is not installed for the custom toolchain 'gnullvm-local'.`
- **Toolchain `stable-x86_64-pc-windows-msvc`:** Falla en la compilación de build scripts y proc-macros (`proc-macro2`, `quote`, `serde_json`, etc.) debido a que no existe `link.exe` de Visual Studio en el sistema (`error: linker link.exe not found`). En este entorno sin admin solo se dispone del toolchain MinGW (`C:\Tools\llvm-mingw-20261104-ucrt-x86_64`).
- **Comprobación honesta:** Las herramientas de linter avanzado (`clippy`) no se encuentran configuradas en el toolchain standalone portable actual.

### 3.3 Warnings de `rustc` (`cargo check --release --workspace --all-targets`)
El compilador genera exactamente 2 advertencias conocidas en el crate raíz:
1. `warning: method wait_ack is never used` en [daemon/src/main.rs:38:8](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/daemon/src/main.rs#L38-L50):
   ```rust
   fn wait_ack(&self) {
       let t = Instant::now();
       let mut b = vec![0u8; self.n + 1];
       while t.elapsed() < Duration::from_millis(1000) { ... }
   }
   ```
   - **Confirmación:** Es **CÓDIGO MUERTO REAL**. Ningún método ni función lo invoca en todo el workspace. Se creó durante el análisis preliminar pensando que el hardware respondía ACK tras cada bloque de imagen.
2. `warning: function screen_off is never used` en [daemon/src/proto.rs:44:8](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/daemon/src/proto.rs#L44):
   ```rust
   pub fn screen_off(n: usize) -> Vec<u8> { crt(n, "HAN", &[(0x0a, 1)]) }
   ```
   - **Confirmación:** Es **CÓDIGO MUERTO REAL**. Ninguna parte del daemon lo invoca (el apagado real usa `handshake_reply` con "HAN" sin CRT o brillo 0).

### 3.4 Metadatos de Cargo

| Crate | `edition` | `publish = false` | `rust-version` | `license` | `lints` |
|---|---|---|---|---|---|
| `orcadeck` (raíz) | `2021` | **FALTA** | **FALTA** | **FALTA** | **FALTA** |
| `orcadeck-builtin` | `2021` | **FALTA** | **FALTA** | **FALTA** | **FALTA** |
| `orcadeck-host` | `2021` | **FALTA** | **FALTA** | **FALTA** | **FALTA** |
| `orcadeck-plugman` | `2021` | **Presente** (L6) | **FALTA** | **FALTA** | **FALTA** |
| `orcadeck-web` | `2021` | **FALTA** | **FALTA** | **FALTA** | **FALTA** |

*Riesgo detectado:* La ausencia de `publish = false` en 4 de los 5 crates expone al repositorio a una publicación accidental en crates.io en caso de ejecutar comandos globales de empaquetado. Se recomienda consolidar mediante `[workspace.package]` en el `Cargo.toml` raíz.

### 3.5 Dependencias duplicadas (`cargo tree -d`) y su coste

El comando `cargo tree -d` identifica 4 paquetes duplicados:

1. **`base64`:**
   - `base64 v0.22.1` (directa en [daemon/Cargo.toml:14](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/daemon/Cargo.toml#L14)).
   - `base64 v0.23.1` (transitiva vía `usvg v0.49` -> `resvg v0.49` en `orcadeck-builtin`).
   - *Coste:* Compilación duplicada del crate y desaprovechamiento de optimizaciones LTO.
   - *Solución:* Subir `base64 = "0.23"` en `daemon/Cargo.toml`.

2. **`getrandom`:**
   - `getrandom v0.2.17` (directa en [daemon/crates/web/Cargo.toml:11](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/daemon/crates/web/Cargo.toml#L11)).
   - `getrandom v0.4.3` (directa en [daemon/crates/host/Cargo.toml:16](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/daemon/crates/host/Cargo.toml#L16), y transitiva vía `rand v0.10` / `tungstenite` / `zip`).
   - *Coste:* Duplica las llamadas al sistema para generación de números aleatorios criptográficos (CSPRNG en Windows `BCryptGenRandom` / `ProcessPrng`).
   - *Solución:* Actualizar `orcadeck-web` a `getrandom = "0.4"`.

3. **`miniz_oxide`:**
   - `miniz_oxide v0.8.9` (vía `png v0.18` en `image` y `tiny-skia`).
   - `miniz_oxide v0.9.1` (vía `flate2 v1.1` en `plugman`, `png`, `zip`).
   - *Coste:* Sobrecarga menor en tiempo de compilación.

4. **`syn`:**
   - `syn v2.0.119` (vía `zeroize_derive v1.5` en `zip`).
   - `syn v3.0.10` (vía `bytemuck_derive`, `serde_derive`, `thiserror-impl`).
   - *Coste:* **Alto en tiempo de compilación**. Parsear dos versiones mayores de la gramática de macros de Rust añade entre 5 y 10 segundos al build frío.

---

## 4. Tests y Cobertura

### 4.1 Inventario de pruebas por crate (Total: 74 tests, 100 % pasando)

- **`orcadeck` (binario principal):** **23 tests unitarios** ([daemon/src/main.rs](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/daemon/src/main.rs)).
  - Cubre: parseo y chunks de protocolo HID (`proto.rs`), validación de dimensiones JPEG y límites de 9000 bytes (`render.rs`), debounce y persistencia de `Settings`/`GlobalSettings` (`app.rs`), filtrado estricto de URLs `http(s)` para `OpenUrl`, orden de escenas y transiciones cíclicas `Scene Shift` (`scenes.rs`), resolución de rutas de perfiles (`edit.rs`).
- **`orcadeck-builtin`:** **6 tests de integración** ([daemon/crates/builtin/tests/port.rs](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/daemon/crates/builtin/tests/port.rs)).
  - Cubre: compatibilidad exacta con `test.js` del plugin legacy de Node, rasterización SVG de tarjetas de agente y panel lateral a 85x85 y 80x80 px con `resvg`, simulación del pipe RPC de Orca y tarjeta de cuota.
- **`orcadeck-host`:** **20 tests** (10 unitarios en `lib.rs`, 8 integración en `fake_plugin.rs`, 1 integración en `real_plugin.rs`, 1 doc-test).
  - Cubre: WebSocket RFC 6455 en loopback, allowlist estricta de `Origin`, generación y consumo único de tokens para Property Inspector, secuencia de backoff exponencial en reinicio de plugins, ciclo `register` / `appear` / `setImage` / `keyDown`.
- **`orcadeck-plugman`:** **15 tests** (2 unitarios en `lib.rs`, 13 integración en `tests/plugman.rs`).
  - Cubre: mitigación de zip-slip (path traversal), mitigación de zip-bombs (ratio 200:1, límites de entradas y descompresión), detección y reconstrucción de overlays para plugins `ELGATO` sin tocar ficheros en disco, exportación e importación de overlays, watcher con debounce de tamaño estable.
- **`orcadeck-web`:** **10 tests** (3 unitarios en `lib.rs`, 5 integración en `tests/api.rs`, 1 integración en `tests/real_folders.rs`, 1 doc-test).
  - Cubre: parser HTTP/1.1 estricto en `std::net`, autenticación por token aleatorio de sesión (`X-OrcaDeck-Token` y cookie `SameSite=Strict`), endpoints REST para gestión de perfiles/celdas, lectura de base de datos SQLite `DataCache.db`, integración contra carpetas reales en modo solo-lectura.

### 4.2 Huecos priorizados de prueba

1. **Caminos de error (Error Paths) — Prioridad Alta:**
   - *Fallo HID en caliente:* Simulación de desconexión USB abrupta durante la transmisión de un lote JPEG de 15 teclas. Hoy `Dock::send` devuelve `Err(String)`, pero falta test de reconexión automática en `app.rs`.
   - *Rotura de Named Pipe de Orca:* Si el proceso de Orca se cierra o reinicia en Windows (`\\.\pipe\orca`), `builtin/src/orca.rs` debe verificar la re-adquisición del token sin pánico ni bloqueo de hilos.
   - *Bloqueo de SQLite:* Cuando `DataCache.db` está abierto por Stream Dock oficial, `sqlite.rs` puede encontrar la base de datos bloqueada (`SQLITE_BUSY`).
2. **Seguridad — Prioridad Alta:**
   - *Path Traversal en iconos de perfiles:* Verificar que `SceneSet::open` y la resolución de imágenes (`.sdProfile/Images/../../`) rechazan rutas relativas que intenten leer ficheros arbitrarios del sistema.
   - *Límites de tamaño en WebSocket:* Comprobar que un plugin que envíe una trama WebSocket gigante (> 10 MB) es desconectado de inmediato para evitar saturación de memoria (OOM).
3. **Concurrencia — Prioridad Media:**
   - *Carrera entre `Scene Shift` y renderizado de plugins:* Cuando el usuario conmuta rápidamente entre escenas mientras plugins en segundo plano emiten `setImage`, comprobar que los contextos antiguos retirados no escriben en la rejilla activa.

### 4.3 Medición del tiempo de gate
- **Comando:** `cargo test --release --workspace` (con toolchain `gnullvm-local`).
- **Tiempo total medido:** **236.986 ms (3 minutos y 57 segundos)** en compilación limpia desde cero con enlazado LTO y optimización release. En ejecuciones incrementales el gate dura ~12 segundos.

---

## 5. Integración Continua (CI): Propuesta de Workflow

Actualmente **no existe el directorio `.github`**. A continuación se propone un workflow mínimo para GitHub Actions (`.github/workflows/ci.yml`) que valida la compilación cruzada con MinGW / LLVM para el target Windows gnullvm utilizado localmente:

```yaml
name: CI

on:
  push:
    branches: [ main ]
    paths:
      - 'daemon/**'
      - '.github/workflows/ci.yml'
  pull_request:
    branches: [ main ]
    paths:
      - 'daemon/**'

env:
  CARGO_TERM_COLOR: always

jobs:
  check-and-test:
    name: Build & Test (Windows gnullvm)
    runs-on: windows-latest
    defaults:
      run:
        working-directory: daemon

    steps:
      - name: Checkout repository
        uses: actions/checkout@v4

      - name: Install Rust toolchain (stable)
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: x86_64-pc-windows-gnullvm
          components: rustfmt

      - name: Install LLVM-MinGW Toolchain
        run: |
          choco install llvm-mingw -y
          echo "C:\tools\llvm-mingw\bin" >> $env:GITHUB_PATH

      - name: Cache Cargo Dependencies & Build
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            daemon/target
          key: ${{ runner.os }}-cargo-gnullvm-${{ hashFiles('daemon/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-gnullvm-

      - name: Check Formatting
        run: cargo fmt --check

      - name: Cargo Check Workspace
        run: cargo check --workspace --all-targets --target x86_64-pc-windows-gnullvm

      - name: Run Test Suite (Release)
        run: cargo test --release --workspace --target x86_64-pc-windows-gnullvm
```

---

## 6. Seguridad e Higiene

### 6.1 Auditoría de Secretos y Credenciales
- **Resultado:** No existen secretos reales, credenciales de producción ni claves privadas comprometidas en el árbol rastreado de git.
- **Manejo de tokens verificado:**
  - `authToken` en [daemon/crates/builtin/src/orca.rs:32](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/daemon/crates/builtin/src/orca.rs#L32): Se lee en tiempo de ejecución desde la tubería local de Orca; no hay tokens cableados.
  - Token de servidor web en [daemon/crates/web/src/api.rs:60](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/daemon/crates/web/src/api.rs#L60): Se genera aleatoriamente con CSPRNG (`getrandom`) de 256 bits en cada arranque del proceso y se compara con `constant_time_eq`.
  - Tokens de Property Inspector en [daemon/crates/host/src/host.rs:188](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/daemon/crates/host/src/host.rs#L188): Tokens de un solo uso por UUID criptográfico.
- **Advertencia higiénica:** En `docs/research/manifest-profiles-admin.md:243` se señala que la base de datos local SQLite de Stream Dock oficial (`OBSServerInfo`) guarda contraseñas de OBS en texto claro en la máquina del usuario; se debe mantener la regla de no copiar ni commitear esa tabla.

### 6.2 Rutas absolutas del usuario en ficheros rastreados
Se han localizado las siguientes rutas personales rastreadas en git:

1. [daemon/crates/builtin/src/actions.rs:87](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/daemon/crates/builtin/src/actions.rs#L87):
   ```rust
   std::env::var_os("QUOTA_DIR").map(PathBuf::from).unwrap_or_else(|| "C:/Users/migue/Github/quota-monitor".into())
   ```
   *Riesgo:* Ruta cableada del usuario `migue`. Si otro usuario o un runner de CI ejecuta las pruebas sin definir `QUOTA_DIR`, intentará buscar esa ruta inexistente.
2. [daemon/crates/host/tests/real_plugin.rs:30](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/daemon/crates/host/tests/real_plugin.rs#L30):
   ```rust
   PathBuf::from(r"C:\Users\migue\Github\orca-dock\com.miguelpicado.orcadock.sdPlugin")
   ```
   *Riesgo:* Test que depende de la existencia de un directorio fuera del repositorio en la máquina del desarrollador.
3. [com.miguelpicado.orcadock.sdPlugin/plugin.js:22](file:///C:/Users/migue/orca/workspaces/orca-stream-dock/repo-audit/com.miguelpicado.orcadock.sdPlugin/plugin.js#L22):
   ```javascript
   const QUOTA_DIR = process.env.QUOTA_DIR ?? 'C:/Users/migue/Github/quota-monitor';
   ```
4. Ficheros de documentación que registran trazas forenses:
   - `docs/HANDOFF.md:3, 22`
   - `docs/STATUS.md:156`
   - `docs/research/manifest-profiles-admin.md:243`
   - `docs/research/firmware-feasibility.md:204`
   - `docs/research/firmware/work/emulation-worker-prompt.md:10`
   - `docs/research/updatetool-analysis.md:24`

### 6.3 Licencias de terceros (`docs/research/community-ideas.md`)

El análisis forense de los 8 repositorios clonados en `/tmp/` impone restricciones jurídicas claras para el desarrollo de `orcadeck`:

| Repositorio | Licencia | Directriz para `orcadeck` |
|---|---|---|
| `mirajazz` | **MPL-2.0** | Copyleft débil por archivo. Se pueden tomar algoritmos o ideas, pero **no copiar código directamente** salvo que el fichero resultante se licencie bajo MPL-2.0. |
| `4ndv_opendeck-akp153` | **GPL-3.0** | **Prohibido copiar código fuente**. Copiar funciones contaminaría el binario `orcadeck` con GPL-3.0. Solo se pueden aprovechar constantes fácticas (mapeo de teclas 1..18, VID/PID, offsets). |
| `Aiacos/ajazz-control-center` | **GPL-3.0** | **Prohibido copiar código fuente**. Solo datos y especificación de protocolos. |
| `Uriziel01_Ajazz-reverse-engineering` | **Sin licencia** | Todos los derechos reservados. No copiar código; solo lectura de observaciones del protocolo. |
| `warleyvic24_akp153e-controller` | **MIT** | Permisiva. Se pueden incorporar fragmentos de código conservando el aviso de copyright original. |
| `kruzif-x_deckhand` | **MIT** | Permisiva. Se pueden adaptar rutinas (p. ej. cálculo de paneles laterales) con atribución. |
| `dvortsis_ajazz-companion-bridge` | **MIT** | Permisiva. Reutilizable con atribución. |
| `radisky_akp153_plugin` | **MIT** | Permisiva. Reutilizable con atribución. |

---

## 7. Plan de Acción Priorizado (P0 / P1 / P2)

> **Regla de concurrencia:** Actualmente 4 workers concurrentes están modificando ficheros dentro de `daemon/src/**` y `daemon/crates/**`. Las acciones que afecten a estos directorios están **marcadas explícitamente como [CHOQUE CON WORKERS]** y deben ejecutarse exclusivamente cuando los workers hayan finalizado y sus cambios estén consolidados.

| Prioridad | ID | Acción Concreta | Comando exacto sugerido | Riesgo | Quién la hace / Ficheros afectados | Choque con workers activos |
|---|---|---|---|---|---|---|
| **P0** | **ACT-01** | **Blindar `.gitignore` para artefactos y leases concurrentes** | Editado `.gitignore` añadiendo `target-*/` y `.opencode/`. | Nulo | Hecho en esta auditoría (`.gitignore`). | **NO (Sin choque)** |
| **P0** | **ACT-02** | **Evitar crash/fallo en CI por rutas absolutas de usuario** | Sustituir fallback de `QUOTA_DIR` por detección dinámica relativa o directorio temporal si la ruta no existe. | Bajo | Worker en `daemon/crates/builtin/src/actions.rs:87`. | **SÍ [CHOQUE CON WORKERS]** |
| **P0** | **ACT-03** | **Aislar test con ruta externa fuera del repo** | Cambiar `tests/real_plugin.rs:30` para que omita el test si no existe la ruta `orca-dock` externa o usar un fixture interno. | Bajo | Worker en `daemon/crates/host/tests/real_plugin.rs:30`. | **SÍ [CHOQUE CON WORKERS]** |
| **P1** | **ACT-04** | **Eliminar código muerto que genera warnings en compilación** | Eliminar método `Dock::wait_ack` en `main.rs:38` y función `screen_off` en `proto.rs:44`. | Muy bajo | Worker en `daemon/src/main.rs` y `daemon/src/proto.rs`. | **SÍ [CHOQUE CON WORKERS]** |
| **P1** | **ACT-05** | **Consolidar metadatos de Cargo y `publish = false`** | Añadir `publish = false`, `rust-version = "1.80"`, y licencia `MIT OR Apache-2.0` en `[workspace.package]` de `daemon/Cargo.toml`. | Bajo | Coordinador o worker tras consolidación de ramas en `daemon/Cargo.toml`. | **SÍ [CHOQUE CON WORKERS]** |
| **P1** | **ACT-06** | **Unificar dependencias duplicadas (`base64` y `getrandom`)** | En `daemon/Cargo.toml` cambiar a `base64 = "0.23"`; en `daemon/crates/web/Cargo.toml` cambiar a `getrandom = "0.4"`. Luego ejecutar `cargo update -p base64 -p getrandom`. | Bajo | Worker en `daemon/Cargo.toml`, `daemon/crates/web/Cargo.toml`. | **SÍ [CHOQUE CON WORKERS]** |
| **P1** | **ACT-07** | **Alinear documentación activa (STATUS vs HANDOFF)** | Actualizar `docs/STATUS.md` para reflejar la integración real del workspace y corregir referencias a UpDateTool en `updatetool-analysis.md` y `firmware-feasibility.md` apuntando a `updatetool-origin.md`. | Nulo | Documentación (`docs/STATUS.md`, `docs/research/updatetool-analysis.md`, `docs/research/firmware-feasibility.md`). | **NO (Sin choque)** |
| **P2** | **ACT-08** | **Incorporar workflow de CI en `.github/workflows/ci.yml`** | Crear `.github/workflows/ci.yml` con el contenido propuesto en la Sección 5. | Bajo | Coordinador tras crear la carpeta `.github`. | **NO (Sin choque)** |
| **P2** | **ACT-09** | **Limpieza / Archivado de ficheros obsoletos** | Mover `deploy.ps1`, `tools/plugin-install/`, y evaluar `com.miguelpicado.orcadock.sdPlugin` hacia `tools/legacy/`. | Bajo | Coordinador (cuando no interfiera con ramas de trabajo). | **NO (Sin choque)** |
| **P2** | **ACT-10** | **Formateo uniforme del workspace (`cargo fmt`)** | Ejecutar `cargo fmt` en todo el workspace tras fusionar las ramas de los 4 workers. | Medio (genera diffs cosméticos extensos) | Coordinador tras merge general de workers. | **SÍ [CHOQUE CON WORKERS]** |
