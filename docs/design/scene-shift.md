# Scene Shift (`com.hotspot.streamdock.profile.rotate`) — diseño para `orcadeck run`

> **Nota (2026-11-10):** implementado en `run` (`daemon/src/scenes.rs`); pendiente la prueba física con
> el teclado. Estado en [`../project/STATUS.md`](../project/STATUS.md).

Investigación 2026-11-10, solo lectura: no se ejecutó la app, ni `orcadeck`, ni se abrió el HID. Etiquetas:
**OBSERVED** = visto en ficheros, bytes o desensamblado; **INFERRED** = deducido, no probado en vivo.
## 1. Qué hace al pulsar (evidencia)
- **OBSERVED**: el plugin integrado de la app es `...\plugins\com.hotspot.streamdock.profile.rotate.sdPlugin\manifest.json`:
  una sola acción, UUID `com.hotspot.streamdock.profile.rotate`, nombre `Scene Shift`, tooltip
  "Switch to another preconfigured scenario", `"PrivateAPI": true` y sin `CodePath`: la implementa la app, no un plugin.
- **OBSERVED** (desensamblado de `Stream Dock AJAZZ.exe`, 3.10.191.0421): `SDActionCanvasLabel::performActionKeyDown`
  (rango `.pdata` `0x140080830`–`0x140083b08`) compara la UUID y, en la rama `profile.rotate`, lee
  `Settings.ProfileUUID`; el salto directo lleva además un índice de página (por defecto 1; el property inspector lee
  `Settings.PageIndex`):
  - `ProfileUUID == "PreviousScene"` o `""` → emite `sigSwitchProfile(deviceUUID, ProfileUUID, serialNumber)`;
    **INFERRED**: el gestor de perfiles resuelve anterior/siguiente. `""` es "siguiente"; `"PreviousScene"` es
    "anterior" (cadenas literales comparadas en el binario; ver Fuentes).
  - cualquier otro valor → compara el destino con el nombre de la escena actual sin `.sdProfile`; si coincide, **cambia
    de página** dentro de la escena; si no, emite `sigSwitchProfile(deviceUUID, destino, PageIndex, serialNumber)`.
    Es decir: un `ProfileUUID` normal es un salto directo a esa escena.
- **OBSERVED**: defaults de la acción en el exe (file offset `0x25C06F8`): `{"DeviceUUID":"","ProfileUUID":""}`.
  Las 4 plantillas `defaultProfiles\AKP153E\en\*.streamDockProfile` traen una Scene Shift en `2,0` con
  `ProfileUUID:""` (siguiente).
- **OBSERVED** en la escena Orca (`R0KZ54Z0-063U-947X-31BI-GG95R8F4HT88.sdProfile`): `2,0` tiene
  `{"DeviceUUID":"","ProfileUUID":"H33G4VTZ-25Z8-PYN7-R34M-ST39SX8TRT2Q"}` (Grabación, `ARROW_RIGHT.png`) y `4,2`
  `{"DeviceUUID":"","ProfileUUID":"3I0U64IN-IT4R-RUB6-EZ4Z-VXS311JFXI05"}` (Monitorización, `ARROW_LEFT.png`).
  Son saltos directos, no "next/previous".
- **INFERRED**: el orden de "siguiente/anterior" es el de las filas de `DataCache.db` para el modelo
  (`SELECT Profile FROM "AKP153E"`), con vuelta al principio/final. No hay ningún `"PreviousScene"` en las escenas
  observadas; falta la prueba en vivo (ver §5).
- El `DeviceUUID` de `Settings` no lo usa este handler (usa el UUID del dispositivo actual); en lo observado siempre
  está vacío. Se ignora.
## 2. Forma de `Settings`
```json
"Settings": { "DeviceUUID": "", "ProfileUUID": "" }
```
- `ProfileUUID`: `""` = siguiente, `"PreviousScene"` = anterior, o id de escena. En lo observado el id va sin sufijo
  (el diseño acepta también `.sdProfile` por robustez).
- `PageIndex` (opcional, no visto en las escenas del usuario): entero, por defecto 1; solo aplica al salto directo y
  a multi-página. `run` carga una página por escena, así que v1 lo ignora.
- `DeviceUUID`: vacío en todo lo observado; sin efecto en el handler.
- `ActionID`, `Controller`, `State` y `States` no cambian: `States[0].Image` suele ser `ARROW_RIGHT.png` /
  `ARROW_LEFT.png` en la escena Orca.
## 3. Dónde viven el orden y la escena actual
- **OBSERVED** orden: `%APPDATA%\HotSpot\StreamDock\config\DataCache.db`, tabla `AKP153E`, columna `Profile` en orden
  de `rowid`: `3I0U64IN…` (Monitorización), `R0KZ54Z0…` (Orca), `H33G4VTZ…` (Grabación).
- **OBSERVED** actual: `config\StreamDockConfig.ini`, `[AKP153E355499441494DeviceConfig] sdProfileName=R0KZ54Z0…sdProfile`
  (fallback `[AKP153EDeviceConfig]`).
- **OBSERVED** el editor web (`daemon/crates/web/src/store.rs`): `FsStore` lee `state.json` (`order`, `current`) y,
  si faltan, los siembra de `DataCache.db`/ini. `PUT /api/scenes/current` y `set_current` escriben
  `%APPDATA%\OrcaDeck\state.json`. `set_order` escribe el mismo fichero.
- **OBSERVED**: `Pages` de cada manifiesto solo se lista a sí mismo en las escenas observadas; no es el orden de escenas.
- Quién actualiza: la app de Stream Dock escribe DataCache/ini; el editor web escribe `state.json`. `run` solo escribirá
  `state.json` (no tocará los ficheros del vendor).
## 4. Diseño mínimo para `run`
Reutilizar `orcadeck_web::FsStore`, que ya es dependencia del daemon: evita duplicar el lector de DataCache/ini y el
estado del editor, no añade dependencias y no arranca ningún servidor. No hay polling nuevo; el cambio solo ocurre al
pulsar.
### 4.1 `daemon/src/scenes.rs` (nuevo)
```rust
pub enum SceneTarget { Next, Previous, Id(String) }
pub struct SceneSet {
    store: orcadeck_web::FsStore, // profiles, plugins, data=%APPDATA%\OrcaDeck, legacy_config=.../config
    profiles: PathBuf,
    order: Vec<String>,           // solo escenas listadas, en orden
    current: String,
}
pub fn target_of(a: &profile::Action) -> Option<SceneTarget>;
impl SceneSet {
    pub fn open(profile: &Path, plugins: &Path, data_root: &Path, device: &str) -> Result<SceneSet, String>;
    pub fn resolve(&self, t: &SceneTarget) -> Option<String>;
    pub fn switch(&mut self, id: &str) -> Result<profile::Scene, String>;
    fn step(&self, delta: isize) -> Option<String>;
}
```
- `open`: `profiles = profile.parent()`, `legacy_config = profiles.parent()/config`,
  `FsStore { profiles, plugins, builtin_plugins: vec![], data: %APPDATA%\OrcaDeck, legacy_config: Some(config) }`,
  `store.scenes()`; `order` = `listed` y `device == device` en orden; `current` = id deducido del argumento CLI (el
  CLI manda; si difiere del store se escribe con `set_current`). Si la escena del CLI no está en `order`, se carga
  igual; para next/prev se usa como posición fuera de lista (next = primera, previous = última).
- `target_of`: solo para `profile.rotate`; `Settings.ProfileUUID` ausente o `""` → `Next`, `"PreviousScene"` → `Previous`,
  otro → `Id(trim_end_matches(".sdProfile"))`.
- `resolve`: `Id` solo si `safe_id` (de `orcadeck_web::store`) y existe `<profiles>/<id>.sdProfile/manifest.json`;
  `Next`/`Previous` con `step(+1/-1)` y `rem_euclid(order.len())`.
- `switch`: `profile::Scene::load`, después `store.set_current(id)` (atómico; si falla, log y seguir), actualiza
  `current`. No cambia `DataCache.db` ni el ini.
### 4.2 `daemon/src/app.rs`
- `enum Target { Builtin(String), Plugin(String), Scene(SceneTarget) }` (derivar `Clone`).
- Refactorizar la colocación inicial en `fn place_scene(...) -> Placed`:
  `Placed { by_key, builtin_keys: Vec<bi::Key>, ctx_keys: HashMap<String,u8>, latest, used: HashMap<String,usize> }`.
  En orden: `profile.rotate` → `Target::Scene`; UUID de built-in conocido (`Registry::uuids`) → `bi::Key` y
  `Target::Builtin(ctx)`; si no, plugin con `host::Instance` + `host.appear` → `Target::Plugin(ctx)`. Placeholder =
  `scene.image_of`; negro si no hay imagen. `run` mantiene además `running: HashSet<String>` y lanza solo
  `used - running` (evita reiniciar plugins vivos).
- `enum SceneCmd { Replace { old: Vec<String>, new: Vec<bi::Key>, ctx_keys: HashMap<String,u8> } }` por canal al hilo
  `builtin`, que hace `registry.disappear`/`appear` y cambia su `ctx_key`. No se comparte el `Registry` con el hilo HID.
- `fn switch_scene(scenes, by_key, latest, host, found, running, builtin_cmd, target) -> Result<Vec<(u8, DynamicImage)>, String>`:
  1. `scenes.resolve(target)`; si no hay destino o es la escena actual, no-op.
  2. `scenes.switch(id)`: carga el manifiesto y persiste `state.json.current`.
  3. Contextos viejos: `host.disappear(ctx)` para cada `Target::Plugin`; `SceneCmd::Replace.old` con los contextos
     built-in.
  4. `place_scene` de la nueva escena (coloca built-ins y hace `host.appear` de los plugins); `host.launch` para los
     plugins que aún no estén arrancados; enviar `SceneCmd::Replace` con los built-in nuevos y `ctx_keys`.
  5. Repintar `old_keys ∪ new_keys`: imagen nueva o negro; actualizar `latest` (para reconexión) y devolver los pares.
  6. `*by_key = new_by_key`.
- `session` pasa a recibir `&mut by_key`, `&mut latest`, `&mut SceneSet`, `&mut running`, `&mpsc::Sender<SceneCmd>`,
  `&[host::Plugin]` y `&HashSet<&'static str>` de built-ins. En `Target::Scene`, llama a `switch_scene`, mete los pares
  en `pending` y sigue; si falla, log y sigue. El switch ocurre en el hilo HID (sin hilo nuevo).
- Pintado: un solo lote con `ORCADECK_GAP_MS` (400 ms por defecto), solo claves de `old ∪ new`, sin `CLE` ni `STP`
  extra. El firmware pierde pulsaciones mientras pinta: mantener la pausa, no bajarla. `latest` permite repintar la
  escena nueva tras reconectar.
- Plugins: `willDisappear` a los contextos viejos y `willAppear` a los nuevos, como en `plugin-host-protocol.md` §routing.
  En v1 no matar procesos que dejan de usarse (pierden `setSettings`/estado al morir); si se quiere memoria mínima,
  retirarlos 2 s después con un `Instant` comprobado en el bucle existente (sin timer nuevo) — opcional.
## 5. Riesgos y preguntas abiertas
- `""`/`"PreviousScene"` está confirmado en el binario, pero no probado en vivo contra el dispositivo; validar con
  Stream Dock parado y una escena de prueba.
- El firmware del AKP153E pierde pulsaciones mientras se pinta; un cambio de 15 teclas tarda ~400 ms y una pulsación
  inmediata puede perderse. No reducir el gap.
- Carrera de pintado: un `tick` de builtin o un `setImage` de plugin viejo puede llegar después del cambio. Etiquetar
  los paints con una generación (`Arc<AtomicU64>`) y descartar los de la generación anterior; para plugins, comprobar
  además que `by_key[key]` sigue apuntando a su contexto.
- Si el `ProfileUUID` es igual a la escena actual, el vendor cambia de **página**, no de escena. En v1 se hace no-op
  (todas las escenas observadas tienen una sola página).
- `Pages` anidados, `PageIndex`, diales, celdas laterales `5,x` y otros modelos (`DeviceUUID` no vacío) quedan fuera.
- `state.json` y el editor web pueden escribirse desde dos procesos; el fichero es atómico y last-wins. El editor no
  cambia la escena en caliente: `PUT /api/scenes/current` no habla con el `run` en marcha. Aceptado en v1.
- No matar plugins usados por la escena anterior evita perder settings, pero mantiene RAM. Medir y decidir si se añade
  la retirada con gracia.
- `switchToProfile` de los plugins sigue sin implementarse en `host`; no es necesario para este built-in.
- Sin probar con hardware en esta tarea (modo investigación). La prueba manual es el paso siguiente.
## 6. Tareas para el implementador
1. **`daemon/src/scenes.rs`**: `SceneTarget`, `SceneSet`, `target_of`; tests de next/previous con vuelta, id explícito
   con y sin `.sdProfile`, id desconocido, escena actual fuera de `order`.
2. **`daemon/src/main.rs`**: `mod scenes;` (el CLI `run <sdProfile>` no cambia).
3. **`daemon/crates/builtin/src/actions.rs`**: `pub fn uuids(&self) -> Vec<&'static str>` en `Registry` para congelar
   los UUID built-in antes de mover el registro al hilo.
4. **`daemon/src/app.rs`**: `Target::Scene`, `place_scene`, `SceneCmd`, `switch_scene` y la nueva firma de `session`.
5. **Pruebas**: `cargo test --release --workspace`; manual con Stream Dock parado:
   `orcadeck run "%APPDATA%\HotSpot\StreamDock\profiles\R0KZ…sdProfile"`, pulsar `2,0` → Grabación, `4,2` →
   Monitorización; una plantilla con `ProfileUUID:""` → siguiente; una con `"PreviousScene"` → anterior;
   `ORCADECK_TRACE=1` para ver un único lote y que la pulsación siguiente no se pierde.
6. **Tamaño**: sin dependencias nuevas; `FsStore` ya está en el workspace. Si se mide el binario, anotar el delta; el
   código nuevo es ~150 líneas y no añade hilos ni polls.
## Fuentes
- `docs/research/profiles/manifest-profiles-admin.md` §2.3–2.5 (formato de escena, DataCache, ini).
- `daemon/crates/web/src/store.rs` (`FsStore`, `scenes`, `set_current`, `set_order`), `api.rs` (`/api/scenes*`).
- `daemon/src/app.rs` (colocación actual, hilos, `by_key`/`latest`), `daemon/src/profile.rs` (`Scene`, `Action`).
- `daemon/crates/host/src/host.rs` (`appear`/`disappear`/`launch`), `docs/research/plugins/plugin-host-protocol.md`.
- `C:\Program Files (x86)\Stream Dock AJAZZ Global\plugins\com.hotspot.streamdock.profile.rotate.sdPlugin\manifest.json`.
- `Stream Dock AJAZZ.exe`: `SDActionCanvasLabel::performActionKeyDown`, strings `ProfileUUID` (file `0x25B4988`),
  `PreviousScene` (`0x25B56D0`), `.sdProfile` (`0x25B56E0`), defaults `0x25C06F8`; desensamblado con Ghidra 12.1.4.
