# Problemas y diagnóstico

> English: [TROUBLESHOOTING.en.md](TROUBLESHOOTING.en.md).

Orden de comprobación rápida, de lo más común a lo más raro. Si el problema es de compilación, ve
directamente a §10.

```powershell
# ¿Ve el teclado? (solo lectura)
.\orcadeck.exe probe

# ¿Quién ocupa el HID o el puerto?
Get-Process orcadeck, node, "Stream Dock AJAZZ" -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 18618 -ErrorAction SilentlyContinue | Select-Object State, OwningProcess

# Log del autoarranque
Get-Content "$env:LOCALAPPDATA\OrcaDeck\orcadeck.log" -Tail 50
```

Regla que prevalece sobre todas: **no flashear el firmware**. Ver §9.

## 1. `-1073741515` al arrancar (libunwind.dll)

- Síntoma: el proceso muere al instante; en PowerShell `$LASTEXITCODE` vale `-1073741515`
  (`0xC0000135`, `STATUS_DLL_NOT_FOUND`).
- Causa: el binario gnullvm necesita `libunwind.dll` y no está en `PATH` ni junto al `.exe`. Pasa al
  ejecutarlo desde el Explorador, una tarea programada o una consola sin el `PATH` de llvm-mingw.
- Arreglo:
  ```powershell
  $env:PATH="C:\Tools\llvm-mingw-20261104-ucrt-x86_64\bin;$env:PATH"
  .\orcadeck.exe probe
  ```
  O copia `libunwind.dll` al lado de `orcadeck.exe`. El script de autoarranque ya lo copia a
  `%LOCALAPPDATA%\OrcaDeck\bin`.
- No es un fallo del código ni del dispositivo.

## 2. «device not found» / `probe` no encuentra nada

Mensaje de `Dock::open`: `device not found (monitors on? Stream Dock closed?)`.

1. **Monitores encendidos**: el teclado cuelga del hub USB del monitor; con los monitores apagados no
   aparece en el sistema.
2. **Stream Dock cerrado**: si la app oficial tiene el HID abierto, `orcadeck` no puede usarlo.
   ```powershell
   schtasks /end /tn StreamDock
   ```
   Si el proceso sobrevive, ciérralo desde una consola elevada.
3. **Cable/hub**: prueba otro puerto USB directo al PC.
4. **Otra instancia de `orcadeck run`**: también tiene el HID.
5. `hidwatch` no abre el dispositivo: úsalo para ver si el USB aparece y desaparece, y para detectar
   `EEEF:2929` (bootloader).

## 3. Stream Dock debe pararse (mismo HID y puerto 18618)

`orcadeck run` y «Stream Dock AJAZZ» no pueden convivir: comparten el dispositivo HID y el puerto
18618 del host de plugins.

- Parar: `schtasks /end /tn StreamDock`.
- Relanzar: `schtasks /run /tn StreamDock`.
- Comprobar: `Get-ScheduledTask -TaskName StreamDock | Select-Object TaskName, State`.
- Si `run` falla al arrancar con `plugin host: …` y el puerto está en `LISTEN`, es casi seguro que
  Stream Dock sigue vivo. Cierra también sus `node20` huérfanos si los hay.

## 4. El puerto 18618 está ocupado

Solo un proceso puede escuchar en `127.0.0.1:18618`. `run` lo usa fijo; `edit` **no**.

```powershell
Get-NetTCPConnection -LocalPort 18618 -State Listen |
  Select-Object LocalAddress, LocalPort, OwningProcess
Get-Process -Id <OwningProcess>
```

- Si el dueño es `Stream Dock AJAZZ` o un `node20` suyo: para la app oficial.
- Si es otro `orcadeck`: ya hay un `run` en marcha; no lances dos.
- `edit` usa un puerto libre (o el que pases con `--port`), así que no colisiona con 18618.

## 5. Se pierden pulsaciones

Es el problema conocido del firmware del AKP153E v1, no de la app.

- Mientras el dispositivo recibe imágenes, **descarta pulsaciones**. Medido: pintar por tecla cada
  ~120 ms dio cero pulsaciones en 80 s.
- Cada imagen cuesta 25–30 ms fijos de escritura HID, aunque pese poco.
- `run` lo mitiga con lotes (todas las teclas pendientes juntas) y un hueco de 400 ms
  (`ORCADECK_GAP_MS`), más JPEG q75.
- Si aún se pierden, **sube** el gap, no lo bajes:
  ```powershell
  $env:ORCADECK_GAP_MS=500
  ```
- Un cambio de escena pinta todas las teclas en un lote; una pulsación inmediata puede caer en ese
  hueco. Es normal.
- Recuerda: el evento llega **al soltar**, no al pulsar. Si pruebas «mantener y mirar», no verás nada
  hasta soltar. No hay pulsación larga ni down/up con este firmware.

## 6. La pulsación llega tarde o la tecla integrada va a tirones

- Las acciones de plugin dependen del plugin; las integradas, del hilo `builtin`.
- Ese hilo llama a Orca: `orchestration.workerList` tarda ~450 ms y se consulta cada 10 s. Durante esa
  llamada el spinner puede avanzar a saltos; es el compromiso documentado para no frenar el bucle del
  HID. Reiniciar `run` no lo mejora.
- Las teclas de agente enfocan un panel de Orca; si Orca está ocupado o la tubería no responde, la
  acción puede tardar o devolver alerta.
- `ORCADECK_TRACE=1` muestra las entradas y los lotes, no el tiempo del RPC.

## 7. Scene Shift no hace nada o cambia a la escena equivocada

1. Comprueba `Settings.ProfileUUID` en el inspector del editor:
   - `""` o ausente → siguiente; `"PreviousScene"` → anterior; otro → id directo.
2. Un id directo debe existir como `<id>.sdProfile\manifest.json` y ser un id seguro. Si no existe,
   `run` escribe `scene: …` en la consola y no cambia nada.
3. Si el destino es la escena actual, es un no-op deliberado.
4. El orden de siguiente/anterior sale de `state.json.order`. Si está vacío o no contiene la escena
   actual, se usa el extremo correspondiente. El editor puede reordenar; `run` lee el orden al
   arrancar.
5. Si has añadido la tecla a la escena, reinicia `run`. Si cambias el destino de una tecla de la
   escena actual, también: `run` no relee esa escena hasta que vuelve a entrar en ella. El destino de
   otra escena se lee al saltar a ella.
6. El cambio pinta todas las teclas afectadas; puede tardar un lote (~400 ms). No lo pruebes con
   Stream Dock en marcha.

Estado: implementado con tests unitarios, **pendiente de prueba física** con el teclado
([ROADMAP](../project/ROADMAP.md)).

## 8. La cuota sale atenuada o con guiones

- `quota-status.json` tiene más de **10 minutos** o no existe: la tecla se atenúa o queda vacía si no
  hay datos.
- `QUOTA_DIR` debe apuntar a la carpeta de `quota-monitor`. Sin definirlo, `run` lee
  `%APPDATA%\OrcaDeck\quota` y la tecla queda vacía. Defínelo antes de arrancar `run`
  (o en la tarea de autoarranque).
- `orca-dock-rs` debe estar en marcha para refrescar el fichero. La tecla de cuota escribe `.wake`;
  `orca-dock-rs` lo mira cada 5 s y refresca. `run` relee cada 3 s hasta 45 s.
- La app es de solo lectura: no inventa ni corrige cifras; si el fichero está mal, el problema es del
  productor.

## 9. `hidwatch` muestra `EEEF:2929` (bootloader)

- `EEEF:2929` es el bootloader del AKP153E; `0300:1010` es la app normal.
- Si aparece el bootloader: el dispositivo está en modo actualización. **No ejecutes
  `FirmwareUpgradeTool.exe` ni envíes `APP NEW`.** Desconecta y vuelve a conectar el USB; si sigue
  así, no hay una recuperación documentada sin intervención de hardware.
- `hidwatch` solo enumera: no abre el dispositivo, no escribe y no flashea. Es el instrumento
  permitido para ver la transición.
- Contexto y riesgo del parche: `research/firmware/patch-emulation.md`. Regla dura: no flashear.

## 10. Fallos de compilación

| Mensaje | Causa / arreglo |
|---|---|
| `linker 'link.exe' not found` o `lld-link not found` | llvm-mingw no está en `PATH`; añade `C:\Tools\llvm-mingw-20261104-ucrt-x86_64\bin` |
| `toolchain 'stable-x86_64-pc-windows-gnullvm' is not installed` | usa `RUSTUP_TOOLCHAIN=gnullvm-local` en esta máquina; no edites `rust-toolchain.toml` |
| `error: failed to run custom build command` en `hidapi` | falta el compilador de C de llvm-mingw o el `PATH`; revisa el paso anterior |
| `cargo test` no encuentra `node` | el test `real_plugin` se omite; no es un fallo |
| tests de `real_folders` omitidos | no hay perfiles reales de Stream Dock; es esperado |
| `-1073741515` al ejecutar tests o el binario | falta `libunwind.dll`; ver §1 |

Comandos:

```powershell
cd daemon
$env:PATH="C:\Tools\llvm-mingw-20261104-ucrt-x86_64\bin;$env:USERPROFILE\.cargo\bin;$env:PATH"
$env:RUSTUP_HOME="%USERPROFILE%\.rustup"
$env:CARGO_HOME="%USERPROFILE%\.cargo"
$env:RUSTUP_TOOLCHAIN="gnullvm-local"
cargo test --release --workspace
```

## 11. El editor no abre o da 403

- `edit` imprime `OrcaDeck editor: http://127.0.0.1:<puerto>/?token=…`. Abre esa URL exacta; el token
  cambia en cada arranque.
- `--no-open` solo evita lanzar el navegador; el servidor sigue escuchando.
- Si el puerto que pasaste con `--port` está ocupado, elige otro o usa el puerto libre por defecto.
- Un 403 suele ser token/Host/Origin: usa la URL recién impresa y no una pestaña vieja.
- El servidor solo escucha en `127.0.0.1`; no es accesible desde otra máquina.

## 12. Los cambios del editor no aparecen en `run`

Con `run` vivo, `edit` corre dentro de `run` y guardar una celda recarga esa tecla en caliente. Si
`edit` arrancó standalone (o el canal `ctl` no está disponible), no hay recarga:

- Reiniciar `run`.
- Cambiar a otra escena y volver (el `SceneSet` recarga el manifiesto destino).
- Para ajustes de plugin, `run` los escribe él mismo con debounce de 1 s; el editor y `run` pueden
  pisarse si editas la misma celda a la vez. Evítalo.

## 13. Un plugin no pinta nada

1. ¿Está el plugin en la escena? Al arrancar, solo se lanzan los plugins usados por la escena activa.
   Además, `run` solo descubre `%APPDATA%\HotSpot\StreamDock\plugins` (u `ORCADECK_PLUGINS`); los
   plugins de `Program Files` se listan en `edit`, pero no se lanzan.
2. ¿El UUID de la acción empieza por el id del plugin (`<plugin>.`)? Si no, `place_scene` escribe
   `no handler for …`.
3. ¿El manifest está cifrado y sin overlay? En **Plugins** aparecerá `needs-manifest`. Instálalo con
   un manifest del fabricante o usa un paquete reconstruible.
4. ¿Es `.html`? No se ejecuta; el host solo lanza `.exe` y `.js`/`.mjs`/`.cjs` con `node`.
5. ¿Falta `node` en `PATH`? El lanzamiento falla y el plugin no aparece.
6. ¿Está conectado el proceso? Si `node` falta o el plugin no arrancó, no habrá imagen; corrige el
   entorno y reinicia `run`.

Los `stderr` de los plugins se descartan (`Stdio::null`). Para el ciclo de vida del plugin, `run`
solo imprime en consola `plugin <id> started` o `plugin <id>: <error>`; el host usa `log`, pero el
binario no instala logger, así que esos mensajes no se ven.

## 14. Consumo alto de CPU o memoria

- El objetivo es 0 % de CPU en reposo; el bucle del HID despierta cada 30 ms y el `builtin` cada
  400 ms, pero el trabajo por despertar es mínimo.
- Si hay CPU alta, mira primero los plugins (sobre todo Node) y el editor abierto en el navegador.
- `edit` en reposo duerme en `accept`; la última medición anotada es ~7 MB RSS y 0 s de CPU.
- Mide antes de optimizar:
  ```powershell
  Get-Process orcadeck | Select-Object CPU, WorkingSet, Id
  ```
- `run` mantiene vivos los plugins de escenas anteriores a propósito; si pesan demasiado, reinicia
  `run`.

## 15. Procesos que quedan vivos

- Al matar `run` (Ctrl+C, cerrar consola, `Stop-Process`), Windows no mata automáticamente los
  procesos hijos de los plugins. Es esperado: el host no instala un cierre ordenado.
- Comprueba `Get-Process node, SystemMonitor` y mata solo los que sepas que eran de OrcaDeck.
- El script de autoarranque mata `orcadeck` antes de reinstalar, pero no los plugins.

## 16. Diagnóstico con traza

```powershell
$env:ORCADECK_TRACE=1
.\orcadeck.exe run <perfil.sdProfile>
```

Salida esperada:

- `device ready, N = 512`
- `in …` por cada entrada HID (con `ORCADECK_TRACE`)
- `painted N keys … B in … ms` por cada lote
- `device: …; retrying in 2 s` si se pierde el USB

Si `painted` tarda mucho más de ~30 ms por tecla o los lotes salen muy seguidos, revisa el gap y los
plugins que más pintan.
