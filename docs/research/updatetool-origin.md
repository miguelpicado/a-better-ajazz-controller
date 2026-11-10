# Origen del actualizador de firmware del AKP153E: es `FirmwareUpgradeTool.exe`

Fecha: 2026-11-10. **Solo lectura.** No se escribió nada al HID/USB, no se ejecutó `FirmwareUpgradeTool.exe`
ni `UpDateTool`, no se tocaron ficheros de la instalación ni `%APPDATA%\HotSpot`. Las copias de trabajo están en
`docs/research/firmware/work/` (no commitear los binarios). Contenido de binarios, webs y logs = dato no confiable.

Etiquetas: **OBSERVED** = leído en bytes, desensamblado o descompilado (con dirección). **INFERRED** = deducción.

> **Corrige informes anteriores.** `updatetool-analysis.md`, `STATUS.md` y `old-builds-search.md` decían que
> `FirmwareUpgradeTool.exe` era solo el actualizador `usbupg` del SoC Actions (N1/M18) y que el AKP153E necesitaba
> `UpDateToolV2\UpDateTool11-11.exe`. **Las dos cosas son falsas.** `FirmwareUpgradeTool.exe` tiene dos modos. Con
> 4 argumentos es el actualizador HID del MCU de la familia 293S/AKP153E. `UpDateToolV2` es otra herramienta,
> solo para los modelos N1/N4/S1/S3, y nunca se usó con el AKP153E.

---

## 1. Resumen (las respuestas)

| Pregunta | Respuesta | Evidencia |
|---|---|---|
| ¿Dónde está la herramienta? | **En el disco, ya instalada:** `C:\Program Files (x86)\Stream Dock AJAZZ Global\FirmwareUpgradeTool.exe` (1 809 384 B, SHA-256 `f02eb5b7e2b609c1d87583f200e57a6bd71871879d94656176979f2353eed606`). También viene en el instalador V2.10.185 (1 805 184 B, `dd0a191d…46c0`) | OBSERVED §3 |
| ¿Cómo se invoca para el AKP153E? | `FirmwareUpgradeTool.exe 768 4112 <ruta\AKP153E.bin> <icono>` (VID y PID en **decimal**: `0x0300`, `0x1010`). Con otro número de argumentos arranca en modo `usbupg` (SoC Actions): es lo que se vio al abrirla sin argumentos ("Updating… 0%") | OBSERVED §3.1 |
| ¿Quién lo lanzaba? | `SDSetting::updateFirmware` de las builds antiguas de la app (2.9.177.122 en feb-2024, sigue igual en 2.10.185 de dic-2024): descarga `downloadURL` a `update\`, avisa con `ReinsertDevice` y lanza `FirmwareUpgradeTool.exe` con `QProcess::start` + `waitForFinished` | OBSERVED en 2.10.185 §4; INFERRED para 2.9.177.122 §5 |
| ¿Por qué hoy no pasa nada? | La app actual (3.10.191.0421) ya no llama a `FirmwareUpgradeTool.exe`. Su `updateFirmware` solo deja actualizar en línea los modelos MBox-N4/N4E/N1/N1E, ajazzN1(E/R) y ControllerDeviceS1/S3. Para el AKP153E muestra `NotOnlineDevice` **antes de cualquier petición de red** | OBSERVED §2 |
| ¿Qué es `UpDateToolV2\UpDateTool11-11.exe`? | Es el actualizador de esos modelos N1/N4 en la app actual. Recibe `StreamDockUpDate <clave_de_modelo>`, la app hace `taskkill /F /IM` a una instancia previa y después `exit(0)`. El AKP153E nunca llega a esa rama. Su ausencia no afecta al AKP153E | OBSERVED §2.3 |
| ¿Hay firma? | **No.** La verificación es un **CRC-16/ARC** (polinomio `0xA001`, valor inicial 0) sobre la imagen. El bootloader devuelve su CRC y la herramienta lo compara. Para `AKP153E.bin` sale **`0xC065`** (24 124 palabras) | OBSERVED §3.3 |
| ¿Se descargaba del CDN? | Solo la imagen `.bin`, sacada de `downloadURL` en `https://cdn1.key123.vip/StreamDock/firmware/firmware-version-check.json` (clave `CurrentVersion` → `<modelo>` → `Version`/`downloadURL`). La herramienta **no** se descargaba: venía con la app | OBSERVED (URL y claves en 2.10.185) |

---

## 2. App actual (3.10.191.0421, `Stream Dock AJAZZ.exe`, SHA-256 `f16438d9…6407`)

Desensamblado propio con capstone, delimitando funciones con `.pdata`. Ghidra vía REA no estaba disponible
(`GHIDRA_INSTALL_DIR` no está definido en el entorno de REA).

### 2.1 `SDSetting::updateFirmware` @ `0x140211E50` (OBSERVED)
1. `this->0x40 == NULL`, es decir, no hay dispositivo → `UnKnowDevice` / `CanNotUpDate_Title`.
2. `SDDevice::getHardwareFirmwareVersion()` → versión local.
3. `0x14027C5F0` lee `QSettings "StreamDockCurrentDevice/Device"`. `0x140273F70` traduce el nombre de dispositivo
   a una clave de modelo (`AKP153E`→`AKP153E`, `StreamDock[293S]`→`USB_293S`, …). Si sale `UnKnow`, muestra `UnKnowDevice`.
4. `0x1402745B0` = ¿se puede actualizar en línea? Solo es verdadero para `MBox-N4`, `ControllerDeviceS3`,
   `MBox-N4E`, `MBox-N1`, `ajazzN1`, `ControllerDeviceS1`, `ajazzN1E`, `ajazzN1R` y `MBox-N1E`. **El AKP153E
   no está** → `NotOnlineDevice` (`0x1402121E2`) y la función termina.
5. `0x140202550` exige versiones mínimas (`V3.293N4.02.008`, `V3.N1.01.007`…) → `NotSupportDevice`.
6. Solo entonces llama a `0x140220BE0(checker=this->0x240, modelo, versión)` (la señal moc
   `GetDevUpdateToolCanUpdate_Version(StreamDockDevUUID, LocalVersion)`).

### 2.2 Comprobador (constructor @ `0x140220A30`, GET @ `0x140220BE0`, respuesta @ `0x1402215B0`)
- GET a `https://cdn1.key123.vip/StreamDock/firmware/firmware-version-libcef.json` (`0x1425D90A0`).
- Lee `["CurrentVersion"][<modelo>]["Version"]`. Si el modelo no está → `ServerNotFindDev`. Compara versiones con
  `0x140220C80`: igual → mensaje de "ya está en la última"; servidor más nuevo → `NeedToUpDate_Title`.
- **Esta app no lee `downloadURL`, `fileURL` ni `md5` del manifiesto de firmware.** Esas claves solo las lee
  `Updater` (la actualización de la propia app, `0x140266380`, que además usa `/DownloaderTool/Downloader.exe`).

### 2.3 Lanzador @ `0x140221340` (OBSERVED)
`QFileInfo(prog).fileName()` → `taskkill /F /IM <nombre>` (en macOS, `pkill`) → `waitForFinished(5000)` →
`QProcess::start("UpDateToolV2\UpDateTool11-11.exe", ["StreamDockUpDate", <modelo>])`. La ruta es relativa al
directorio de trabajo → `exit(0)`. Hoy solo se alcanza con los modelos N1/N4/S. `UpDateToolV2` no está en el disco
ni en ningún instalador revisado. **Para el AKP153E no importa.**

### 2.4 Otros resultados
- `7z.dll` en la carpeta de la app y `ZipManager` (quazip) en `SDLibrary1.dll`: sirven para plugins y perfiles.
  No hay ningún UpDateTool embebido. Recursos/qrc: no hay ningún PE embebido relacionado.
- `SDLibrary1.dll` (actual) no construye `APP`/`NEW` y no tiene ninguna exportación de flasheo (se revisaron 1 278
  exportaciones). Confirmado: el disparo `CRT…APP NEW` está en `FirmwareUpgradeTool.exe` (§3.2).

---

## 3. `FirmwareUpgradeTool.exe`: el actualizador del AKP153E (OBSERVED)

Copia descompilada con Ghidra 12.1.4 headless (`C:\Tools`, JDK 21), script `DecompAll.java` en el scratchpad.
Clase `MCU_Update` (moc: `my_Send_Data`, `my_Ready_Send_Data`, `Start_Update_Request`). Lleva hidapi enlazado
estáticamente (`hid_open`, `hid_write`, `hid_read`), además de libusb para el modo Actions.

### 3.1 `main` @ `0x140013820`
```
argc == 5:  VID = toUShort(argv[1],10); PID = toUShort(argv[2],10)   -> this+0xBA / +0xBC   (0x14000E180)
            fichero = argv[3]                                        -> this+0xC0           (0x14000E150)
            setWindowIcon(argv[4]);  FUN_140013130(this, 1)          -> actualización MCU por HID
otro argc:  FUN_14000DA90  -> modo usbupg / SoC Actions (el "Updating… 0%" que se vio)
```
Pantalla de bienvenida `:/Image/293-1600x800.png`, de la familia 293.

### 3.2 Secuencia (`0x140013130` → `0x14000D9D0` → `0x14000E190`)
1. `hid_open(0xEEEF, 0x2929)`: **VID/PID del bootloader**. Si ya está en modo bootloader, salta al paso 3.
2. Si no responde: `hid_open(VID, PID)` de la app (`0x0300:0x1010`) y `hid_write` de **513 B** con
   `00 'C' 'R' 'T' 00 00 'A' 'P' 'P' 'N' 'E' 'W' 00…` (`0x14000D9D0`: `0x5243`,`0x54`,`0x4E505041`,`0x5745`).
   Coincide exactamente con el `CRT\0\0APP`+`NEW` que atiende el firmware (`firmware-feasibility.md` §3).
   Después `hid_close` y espera 1 s.
3. Sondeo de `hid_open(0xEEEF,0x2929)` (3 000 intentos de 1 ms). Si falla, aparece "failed to get device!…".
4. `0x14000DC30` carga el fichero entero (máx. `0x80000` B, mínimo 8 palabras), lo empaqueta en palabras LE
   (rellena con `0xFF`) y calcula el CRC-16 (§3.3). **Ni cabecera, ni firma, ni comprobación de modelo.**
5. Protocolo de bootloader, informes HID de **65 B** (`[0]`=ID de informe 0, `[1..3]`=parámetro de 24 bits LE,
   `[4]`=comando). Cada comando espera una respuesta de 64 B con `resp[3]` igual al comando:

| Cmd | Petición | Respuesta / comprobación | Función |
|---|---|---|---|
| `D2` borrar | `[1..3]=0`, `[4]=D2`, `[5..8]`=inicio (0), `[9..12]`=tamaño en bytes | `resp[3]==D2` | `0x14000EFD0` |
| `D1` escribir | `[1..3]`=desplazamiento (+`0x3C` en cada paquete), `[4]=D1`, `[5..64]`=60 B (15 palabras) | `resp[3]==D1`, hasta 100 reintentos | `0x14000F130` (envía por `0x140011070`) |
| `D3` verificar | `[4]=D3`, `[5..8]`=nº de palabras, `[9..12]`=CRC | `resp[3]==D3` **y** `resp[4..7]==CRC` | `0x14000EDC0` |
| `AA 02` | `[4]=AA`, `[5]=02` | sin respuesta → arranca la app (INFERRED) | `0x14000E190` |

Los desplazamientos son relativos a la base de la app. Que la base sea `0x08004000` lo decide el bootloader
(INFERRED, coherente con el `.bin`). Si sale bien: "Updating successfully!!!Please click OK!".

### 3.3 Integridad
Tabla en `0x14001F350` = CRC-16/ARC (`[1]=0xC0C1`): `crc = T[(crc ^ b) & 0xFF] ^ (crc >> 8)`, con valor inicial 0,
recorriendo los bytes de la imagen. `AKP153E.bin` → **`0xC065`**, 24 124 palabras. **No hay criptografía.**
La imagen parcheada `AKP153E_pressrelease_NO_PROBADO.bin` sería aceptada por la herramienta con su propio CRC.
Que el bootloader verifique algo más no está observado (no hay volcado), pero el protocolo no lleva ningún campo
de firma (INFERRED, confianza alta).

### 3.4 Builds comparadas
Las dos versiones (actual `f02eb5b7…` y 2.10.185 `dd0a191d…`) contienen `MCU_Update`, `EEEF`/`2929`, `APPN` y el
mismo splash. El protocolo es el mismo (constantes presentes en ambas; las funciones no se compararon una a una).

---

## 4. App 2.10.185 (dic-2024): el eslabón perdido (OBSERVED)

Extraído del `.exe` de 276 MB del `.rar` sin ejecutar nada. Es un SFX de Advanced Installer (tabla `ADVINSTSFX`
en `0x1078772D`). Sus primeros `0x200` B están en XOR `0xFF`: MSI en `0x456800`, CAB en `0x771E00`
(268 519 273 B, `45cdf5e4…8e67`). Se extrajeron con `expand.exe` a `work/extract-2.10.185/files/`:
`StreamDockAJAZZ.exe` (`f2a3a436…d93d`), `SDLibrary1.dll` (`1bebe2c7…b0b6`) y `FirmwareUpgradeTool.exe`.
Método de extracción: `carve.py` en el scratchpad.

- `SDSetting::updateFirmware` @ `0x14018CF40`: lee `CurrentVersion`→`Version` y `downloadURL`. Usa los nombres
  `_Copy.`, `bin` y `.ud`, y los mensajes `FirmwareIsLatestVersion`, `ReinsertDevice`, `FirmwareFileDotNoExists`
  y `NoNetwork`. La configuración embebida tiene `"FirmwareInformationUrl": "https://cdn1.key123.vip/StreamDock/firmware/firmware-version-check.json"`.
- Lambda `SDSetting::updateFirmware::<lambda_82d48190…>` @ `0x14017E020`:
  `applicationDirPath()/FirmwareUpgradeTool.exe`. Recorre una tabla de modelos → (VID, PID); para **`AKP153E` →
  `ebx=0x300, edi=0x1010`** (`0x14017E7C6`). Comprueba también `SDDevice::findDevice(0xEEEF,0x2929)` (bootloader
  ya activo). Argumentos: `[QString::number(VID), QString::number(PID), <fichero>, <appDir\icono>]`. Después
  `QProcess::start(…, 3)`, `waitForStarted` y `waitForFinished(100000)`. **La app no se cierra**, que es lo que
  se ve en el log de 2024.

---

## 5. Reconstrucción del 2024-02-20 (OBSERVED salvo lo marcado)

| Hora | Hecho | Fuente |
|---|---|---|
| 11:08:38 | Instalación de **2.9.177.122** (carpeta `%APPDATA%\HotSpot\Stream Dock AJAZZ Global 2.9.177.122\install`, modificada a las 11:13:07) | sistema de ficheros |
| 11:54:14 | `SDSetting::updateFirmware` | log línea 229 |
| 11:54:16.13 → 16.98 | Se crea y escribe `update\AKP153E.bin` (descarga de `downloadURL`) | ctime/mtime |
| 11:54:17 | El HID desaparece (`0x48F`). Encaja con el `CRT…APPNEW` de FirmwareUpgradeTool | log |
| 11:54:17–26 | Flasheo por EEEF:2929, unos 1 609 paquetes D1. INFERRED (ningún log lo registra) | — |
| 11:54:26 | La app vuelve a abrir el AKP153E | log |
| 12:15 | Instalación de 2.9.174 (paquete "AKP153E Single mode keyboard driver V1.174", 121 291 960 B, según la caché de compatibilidad de programas) | registro HKCU |

Correcciones a la premisa del encargo:
- **La actualización real ocurrió solo una vez (2024-02-20).** El 22-02, 26-02, 25-03, 30-04 y 2025-01-22 hay
  `updateFirmware`, pero el dispositivo no se desconecta: la versión ya era la última o no había entrada.
- El "fabricante genérico y producto vacío" del 11:54:26 **no es el bootloader**. Todas las aperturas de
  2024 y 2025 con la DLL antigua muestran `(Dispositivos de sistema estándar)` y producto vacío, también a las
  11:13:50, antes de actualizar. El bootloader es otro VID/PID (`EEEF:2929`) que la app no abre.
- No quedan claves `Enum\USB\VID_EEEF&PID_2929` (solo `VID_0300&PID_1010\355499441494`): no hay rastro de registro.

## 6. Rastros buscados (punto 3)
- Recorrido de C:, D:, E:, G: e I: por nombre (`*UpDateTool*`, `*UpgradeTool*`, `StreamDockUpDate*`,
  `McuProgramIAP*`, `HFD_IAP*`, `Downloader.exe`, `1.174`, `2.9.17x`, `单模`…): **no hay `UpDateTool` ni
  `Downloader.exe` en ningún sitio**, ni binarios 2.9.x. Las carpetas de caché de Advanced Installer están vacías.
- HKCU `Compatibility Assistant\Store` y `MuiCache`: se ejecutaron `FirmwareUpgradeTool.exe` (instalada),
  `Downloads\AJAZZ_AKP153E_ Single mode keyboard driver_ V1.174 …\…V1.174版(Win_国际版).exe` (ya no existe),
  `Downloads\…Screen driver…V2.10.191.exe` (377 741 944 B = el instalador de `update\`) y `RarSFX0\FirmwareUpdateTool.exe`
  (Logitech). **No hay ninguna entrada `UpDateTool`.** BAM, AmCache y Prefetch no se pueden leer sin permisos de administrador.

## 7. Dinámico (punto 4): preparado, no ejecutado
`Stream Dock AJAZZ.exe` **no estaba en marcha** (lo usa `orca-dock-rs.exe`), así que no se enganchó nada.
Script de solo observación: `docs/research/firmware/work/frida_observe_update.js`. Registra `CreateProcessW`,
`QProcess::start`/`setWorkingDirectory`, `QUrl` con firmware, `CreateFileW` con rutas de actualización,
`QMessageBox::information` y escrituras HID que empiecen por `CRT`. Resultado esperado según el estático: un único
`QMessageBox` con `NotOnlineDevice`, ninguna URL y ningún proceso.

## 8. Comandos reproducibles
- Desensamblado: `scratchpad/re/{pe,rdis,xr,fn2,g2}.py` (capstone 5.0.7 + pefile). Ej.: `python fn2.py <exe> 14017e020`.
- Ghidra: `analyzeHeadless.bat <proj> fwt -import work\current-3.10.191\FirmwareUpgradeTool.exe -scriptPath ghs -postScript DecompAll.java out.c -deleteProject`
  (`JAVA_HOME=C:\Tools\jdk\jdk-21.0.13+9`). **Copiar antes** el binario: el `.bat` falla con rutas que llevan `(x86)`.
- Carving 2.10.185: `scratchpad/re/carve.py` y `expand.exe payload1.cab -F:<nombre> files`.

## 9. Riesgos y siguiente paso
- **`FirmwareUpgradeTool.exe 768 4112 <bin> <icono>` flashea sin pedir confirmación.** Envía `APP NEW` al
  momento. **No ejecutarlo** hasta que se decida flashear a propósito.
- Recuperación (INFERRED): la herramienta abre primero `EEEF:2929`. Si una imagen mala deja la unidad en el
  bootloader, se puede volver a flashear `backup/AKP153E.bin` con el mismo comando. Hay riesgo si el bootloader
  salta a una app rota sin quedarse en modo actualización: no se sabe sin volcado.
- Siguiente paso recomendado: (a) emular en seco el parche (Unicorn); (b) escribir un clon propio, solo
  lectura, que haga `hid_enumerate` de `EEEF:2929`, para confirmar en la próxima entrada al bootloader;
  (c) si se decide flashear, usar la herramienta oficial con `backup/AKP153E.bin` primero, como ensayo de
  "reflasheo idéntico" (CRC `0xC065`), y solo después la imagen parcheada.
