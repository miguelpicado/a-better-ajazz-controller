# UpDateTool / mecanismo de actualización del AKP153E — análisis estático

Fecha: 2026-11-10. **Análisis estático, solo lectura.** No se ejecutó ningún binario (ni el
instalador, ni UpDateTool, ni los binarios extraídos, ni la app Stream Dock). No se abrió el HID,
no se escribió al dispositivo, no se flasheó nada. Todo lo extraído se trató como dato no confiable.

Etiquetas: **OBSERVED** = leído en bytes/código/API, con offset o dirección. **INFERRED** = deducción
sin prueba directa. Direcciones `0x080…`/`0x200…` = firmware AKP153E. `0x14…` = `Stream Dock AJAZZ.exe`.
`0x18…` = `SDLibrary1.dll`. Offsets `0x00…` = offset de fichero del binario correspondiente.

---

## 0. Ficheros y hashes (todos OBSERVED salvo nota)

| Fichero | Tamaño | SHA-256 | Nota |
|---|---|---|---|
| `%APPDATA%\HotSpot\StreamDock\update\Stream-Dock-AJAZZ-Installer_Windows_global.exe` | 377 741 944 B | `61b104dc…1625` (registrado; tamaño verificado) | Instalador de la app. Localizado. No se ejecutó |
| `C:\Program Files (x86)\Stream Dock AJAZZ Global\Stream Dock AJAZZ.exe` | 41 226 728 B | — | EXE instalado. Contiene `SDSetting::updateFirmware` |
| `…\Stream Dock AJAZZ Global\SDLibrary1.dll` | 10 934 248 B | — | Capa HID (hidapi). NO flashea |
| `…\Stream Dock AJAZZ Global\FirmwareUpgradeTool.exe` | 1 809 384 B | — | Para el SoC Actions (familia d13x), **no** AKP153E |
| `UpDateToolV2\UpDateTool11-11.exe` | — | — | **Ausente en todo el disco y en el instalador** (ver §1) |
| `backup/AKP153E.bin` = `update\AKP153E.bin` (idénticos) | 96 496 B | `15e03f5b…4f6f` | Firmware app actual. SP=`0x200321C0`, Reset=`0x08004159` |
| `work/AKP153E_pressrelease_NO_PROBADO.bin` | 96 496 B | `a7ea1549…48fc4` | Parche NO PROBADO. 50 bytes distintos, sin cambio de tamaño |
| `C:\Users\migue\Downloads\AJAZZ_ak820_pro_2.4G_upgrade.zip` | 863 766 B | `dffb09a6…6351` | Herramienta oficial de OTRO dispositivo (referencia) |
| ↳ `2.4G_upgrade.exe` (dentro del zip) | 4 343 808 B | `02991598…e38d31` | `HFD_IAPTool_230823`, PE32 x86 MFC |
| `<repo>\Stream-Dock-AJAZZ-Installer_Windows_global.exe` (2º lote, raíz) | 560 933 008 B | `8e91648f…1abe9` | **Distinto** del de 377 MB (`61b104dc…1625`). Build mayor/nuevo. Ver §9 |
| `<repo>\AJAZZ_AKP153E&AKP815&AKP03E_Screen Driver (…) _V2.10.185.rar` | 263 816 499 B | `9a2b7765…124f` | RAR5. Contiene 1 exe anidado. Ver §9 |

---

## 1. ¿Existe el UpDateTool? ¿Dónde vive? (OBSERVED)

**No está disponible para analizar.** Resultados concretos:

| Comprobación | Resultado |
|---|---|
| `UpDateTool*` en disco (Program Files, %APPDATA%, %LOCALAPPDATA%, %TEMP%) | **0 coincidencias reales** (solo carpetas `FirmwareUpdateTool` de Sentry, no relacionadas) |
| Cadenas `UpDateTool` / `UpDateToolV2` / `UpDateTool11-11` / `StreamDockUpDate` en el instalador de 377 MB (ASCII **y** UTF-16LE, fichero completo) | **0** |
| Las mismas cadenas en `Stream Dock AJAZZ.exe` | **Presentes**, agrupadas en `0x25d81e8` = `UpDateToolV2\UpDateTool11-11.exe`, `0x25d8210` = `StreamDockUpDate` |
| Tipo del instalador | PE (MZ/PE en `0x120`); instalador **propio tipo Qt** (cadena `Qt`, `Install`, `Company` en recursos). No es NSIS (firma `NullsoftInst` ausente), ni Inno, ni 7z, ni CAB |
| Payload extraíble del instalador | Solo un ZIP embebido (offset `0x4bcae0e`) = **caché de tienda** (`Images/*.mbg/.gif/.png` + `manifest.json`, 113 cabeceras locales, sin directorio central). **No contiene UpDateTool** |

**Flujo real de la app** (OBSERVED, cadenas de `SDSetting::updateFirmware` `0x14…`, cluster `0x25d6330`→`0x25d8210`):
descarga `https://cdn1.key123.vip/StreamDock/firmware/firmware-version-libcef.json` → compara
`CurrentVersion`/`ServerVersion`/`LocalVersion` (si no está el modelo → `ServerNotFindDev`) → `taskkill`/`pkill`
→ lanza **`UpDateToolV2\UpDateTool11-11.exe StreamDockUpDate`** por ruta relativa al directorio de la app.

> **Conclusión §1:** el EXE **delega** el flasheo en `UpDateTool11-11.exe`, que **no se instala con este build
> ni viene como cadena en el instalador**. Versión, firma Authenticode, dependencias (hidapi/libusb/WinUSB),
> soporte de PID `0x1010` y otros modelos de UpDateTool son **NO DETERMINABLES** (binario ausente).
> El payload principal del instalador (EXE/DLL/UpDateToolV2) está comprimido en el formato propio del instalador
> Qt, que **no es un contenedor estándar extraíble** con las herramientas presentes (no hay 7-Zip/innoextract;
> el ZIP embebido es solo la tienda). Para el protocolo definitivo hace falta obtener `UpDateToolV2` de AJAZZ/Mirabox.

---

## 2. ¿Lo hace la app en el propio proceso? (re-examinado por petición — OBSERVED)

La hipótesis de "la actualización ya está en la instalación, sin herramienta externa" se comprobó contra
`SDLibrary1.dll` (la capa que habla HID con el dispositivo, direcciones `0x18…`):

| Comprobación en `SDLibrary1.dll` | Resultado |
|---|---|
| Única cadena `.bin` | `tmpGifBin.bin` (`0x17f798`) → buffer temporal de GIF, **no firmware** |
| `updateFirmware` / `UpDateTool` / `StreamDockUpDate` | **0** (solo existen en el EXE) |
| Construcción de `APP` / `NEW` / `CRT\0` como literales | **0** (el EXE sí tiene `CRT\0`, 9 veces; ni EXE ni DLL construyen `APP`/`NEW`) |
| `bootloader`, ruta `update\…\.bin`, `sendFirmware`/`writeFirmware` | **0** |
| Lo que **sí** hay | `Get firmware version :` (`0x181640`), `FirmwareVersion(` (`0x181e4b`), `openHidDevice`/`closeHidDevice`, `hid_get_*` → **solo lee la versión** y hace el protocolo HID de pantalla/teclas |

> **Conclusión §2:** `SDLibrary1.dll` **no flashea**, no transfiere la imagen y no construye `APP`+`NEW`.
> El informe previo que decía "la app no construye APP/NEW y delega" queda **reconfirmado**, no refutado.
> Que la unidad llegara a `V2.293S.00.003` (log `2025-06-10`) es compatible con que `UpDateTool11-11.exe`
> **estuviera presente en el momento de actualizar y se quitara/no se conservara** después, o con que esa línea
> de log sea solo la lectura de versión al arrancar (INFERRED). Los logs actuales (`debug.log` = CEF/Chromium;
> los logs de firmware citados antes ya no existen) **no registran** el proceso de flasheo.

---

## 3. Protocolo de transferencia al bootloader (parcial — lo verificable)

Con UpDateTool ausente, la **secuencia HID exacta hacia el bootloader del AKP153E no es reconstruible**
(tamaño de paquete, VID/PID en modo bootloader, formato de bloque, direcciones, verificación). Lo conocido:

| Elemento | Estado | Evidencia |
|---|---|---|
| Disparo de actualización | `CRT\0\0APP` con `[8..10]="NEW"` → el firmware de la app pone `[0x0807E010]=0x21436587` y resetea (dispatcher `0x08006870`, flash `0x08004C4C`) | OBSERVED (firmware, informe previo) |
| Quién envía ese disparo | **Desconocido**: ni EXE ni DLL lo construyen → lo haría UpDateTool | OBSERVED (ausencia) |
| Recepción de la imagen | La hace el **bootloader** `0x08000000–0x08003FFF`, que **no está en el `.bin`** | INFERRED |
| VID/PID en modo bootloader, bloque, CRC/firma, versión | **NO DETERMINABLE** | — |

---

## 4. Formato esperado de la imagen (OBSERVED, verificado ahora)

| Campo | Valor |
|---|---|
| Tipo | Binario plano Cortex-M; empieza por la tabla de vectores (SP=`0x200321C0`, Reset=`0x08004159`) |
| Base | `0x08004000` (solo aplicación; fuera quedan bootloader y sector config `0x0807E000`) |
| Tamaño | 96 496 B exactos |
| Cabecera propia / cifrado / compresión del code | Ninguna / no / no (solo los datos RW van LZ-comprimidos por `armlink`) |
| **Checksum/CRC/firma dentro de la imagen** | **Ninguno.** Los 16 bytes de cola son idénticos en original y parche; el parche cambia **50 bytes** en `0x08006F92–0x0801ABBF` **sin alterar el tamaño ni añadir cola** → no hay checksum que recalcular | 

> Comparado byte a byte: `update\AKP153E.bin` == `backup/AKP153E.bin` (sha `15e03f5b…`). La imagen que
> flashearía la ruta oficial es exactamente el firmware actual.

---

## 5. ¿Validación criptográfica? (OBSERVED donde hay binario)

- **En UpDateTool:** NO DETERMINABLE (ausente).
- **En la imagen:** no hay firma ni hash embebidos (§4).
- **En el bootloader:** desconocido (no está en el `.bin`).
- **Referencia del mismo proveedor (AK820, §7):** el motor de actualización `McuProgramIAP_*` **no tiene ningún
  símbolo criptográfico** (0 coincidencias de RSA/ECDSA/AES/SHA/sign/signature); su única verificación es
  `McuProgramIAP_Verify`, que por nombre y contexto es **CRC/relectura**, no firma.

> **Inferencia (confianza media):** el ecosistema de actualizadores de este proveedor **no firma** las imágenes;
> verifica por CRC/relectura. Si el bootloader del AKP153E sigue el mismo patrón (solo CRC/tamaño), **una imagen
> parcheada sin firma sería cargable**. **No confirmado** para el AKP153E: depende del bootloader, que no se ha
> volcado. Es exactamente la **PARADA 1** del plan previo.

---

## 6. Recuperación (sin cambios respecto al informe previo — INFERRED)

Reentrada al bootloader si la app falla: desconocida sin la hoja de datos del MCU (C\*Core, Cortex-M4F).
Vías de último recurso probables: ISP por pin BOOT al enchufar, o SWD (SWDIO/SWCLK) si no está bloqueado.
Imagen inválida → riesgo de brick **medio-alto** sin SWD; **bajo** con SWD y volcado completo (incluido bootloader
y `0x0807E000`). No se ha abierto la unidad ni verificado pads.

---

## 7. AK820 vs UpDateTool (referencia comparativa)

`2.4G_upgrade.exe` (dentro de `AJAZZ_ak820_pro_2.4G_upgrade.zip`) es el actualizador oficial del **receptor 2.4G
del teclado AK820 Pro**. Es útil como **modelo del diseño de actualizador del proveedor**, no como herramienta
reutilizable para el AKP153E.

| Aspecto | AK820 `2.4G_upgrade.exe` (OBSERVED) | UpDateTool del AKP153E |
|---|---|---|
| Transporte | **HID Win32 directo**: `SETUPAPI` (`SetupDiGetClassDevs`/`EnumDeviceInterfaces`/`GetDeviceInterfaceDetail`) + `HID.DLL` (`HidD_GetAttributes`, `HidP_GetCaps`) + `CreateFileW`/`WriteFile`/`ReadFile`. **Sin libusb/WinUSB** | Desconocido (ausente). La app AKP153E usa hidapi+libusb |
| Motor | API con nombres `McuProgramIAP_*` (`HFD_IAPTool_230823`, build 2023-08-23) | Desconocido |
| Secuencia de flasheo | `SearchDev`→`SetVIDPID`→`SetChannel`/`SetPower`/`SetCapacitance`→`Erase`→`Program`→`Verify`→`GetProgress`→`Restart`/`CloseDevice` | Desconocida |
| Parametrización | **Genérica**: `SetVIDPID` (VID/PID por parámetro), canal RF, potencia; firmware elegido por el usuario (`COMDLG32 GetFileTitle`) | — |
| Firma criptográfica | **No** (0 símbolos cripto; solo `Verify`=CRC/relectura) | Desconocida |
| Chip objetivo | MCU del **receptor 2.4G HFD** (no Cortex-M) | AKP153E = Cortex-M4F C\*Core |

**Lectura:** el motor es **parametrizable por VID/PID + fichero de firmware**, lo que *sugiere* reutilización…
pero el **conjunto de comandos del bootloader es específico del chip HFD** del receptor 2.4G, **distinto** del
Cortex-M4F del AKP153E. Cambiar el VID/PID **no** hace compatible el protocolo de bootloader. Además, la DLL del
motor (`McuProgramIAPDll`) **no viene en el zip** (solo el `.exe`), así que los bytes exactos de la transferencia
tampoco están en claro aquí. **No es un sustituto del UpDateTool del AKP153E**, pero confirma el patrón del
proveedor: **actualizador HID por IAP, sin firma, verificación por CRC**.

---

## 9. Segundo lote: instalador de 561 MB y `.rar` V2.10.185 (OBSERVED)

El usuario aportó dos artefactos nuevos en la raíz del repo para buscar `UpDateToolV2`. Resultado: **no
aparece en ninguno por los medios disponibles.**

**(a) `Stream-Dock-AJAZZ-Installer_Windows_global.exe`, 560 933 008 B, SHA-256 `8e91648f…1abe9`.**
Es un fichero **distinto** del de 377 MB (`61b104dc…1625`) — otro build, no una copia. Mismo tipo (instalador
propio Qt).

| Comprobación | Resultado |
|---|---|
| Cadenas `UpDateTool`/`UpDateToolV2`/`UpDateTool11-11`/`StreamDockUpDate`/`McuProgramIAP`/`IAPTool`/`HFD_IAPTool` (ASCII+UTF-16, fichero completo) | **0** |
| Cadenas de front-end (sin comprimir, offsets `0x14…0x75…`) | `CRT` (73), `.bin` (6), `libusb` (5), `AKP153E` (4), `FirmwareUpgradeTool` (3), `hidapi` (2) — igual que el de 377 MB |
| ZIP embebido (433 cabeceras locales `PK\x03\x04`) | Solo contenido de tienda: `manifest.json` + `profiles/*.sdProfile/Images/*` (gif/png/mbg). **Sin UpDateTool, sin firmware `.bin`, sin binarios de programa** |
| Payload principal (EXE/DLL/UpDateToolV2) | En el formato comprimido propio del instalador Qt → **no extraíble** con lo presente (sin 7-Zip/QtIFW/innoextract) |

**(b) `AJAZZ_AKP153E&AKP815&AKP03E_Screen Driver (Win_International version) _V2.10.185.rar`, 263 816 499 B,
SHA-256 `9a2b7765…124f`.** RAR5. Parseando las cabeceras RAR5 a mano (solo lectura):

| Entrada | Tipo | Método | Tamaño descomprimido | Nombre |
|---|---|---|---|---|
| 1 | FILE | m3 (comprimido RAR) | 276 355 256 B | `AJAZZ_AKP153E&AKP815&AKP03E_屏幕驱动（Win_国际版） _V2.10.185.exe` |
| 2 | SERVICE | store | 126 B | `QO` (QuickOpen) |

Contiene **un único `.exe` anidado** (otro instalador de pantalla, build V2.10.185), comprimido con el algoritmo
propio de RAR. **No se pudo extraer:**
- No hay 7-Zip, WinRAR ni `unrar` instalados.
- `bsdtar`/libarchive 3.8.9 (único extractor presente) **falla** sobre este RAR por su registro de servicio
  QuickOpen (`Archive entry has empty or unreadable filename … skipping` → error), y no extrae nada.
- Aun extrayéndolo, el `.exe` resultante sería **otro instalador Qt** que requeriría el mismo utillaje ausente.

> **Conclusión §9:** `UpDateToolV2`/`UpDateTool11-11.exe` **no se encuentra** en el instalador de 561 MB (ni como
> cadena ni en su ZIP de tienda) y **no se puede comprobar** dentro del `.rar` sin un extractor RAR real. El
> bloqueo es de **utillaje**, no de evidencia. **Propuesta (no ejecutada, requiere tu visto bueno):** instalar
> **7-Zip** (abre el RAR y suele poder abrir los datos de componente del instalador Qt) o **WinRAR/UnRAR** en una
> máquina de análisis y repetir; alternativamente, pedir `UpDateToolV2` directamente a soporte AJAZZ/Mirabox. No
> se instaló nada. Los originales no se movieron ni se borraron.

---

## 8. Veredicto

**No flasheable hoy de forma segura.** El parche (`work/AKP153E_pressrelease_NO_PROBADO.bin`) es correcto de
formato (binario plano, 50 bytes en zona de código, sin CRC/firma que recalcular), pero **falta el medio de
carga**: `UpDateTool11-11.exe` no está en el equipo ni (como cadena) en el instalador, `SDLibrary1.dll` no
flashea, y el bootloader del AKP153E no está volcado, así que su protocolo y su verificación son desconocidos.

| Aspecto | Valoración |
|---|---|
| ¿Herramienta de flasheo disponible? | **No** (UpDateTool ausente; no extraíble del instalador con lo presente) |
| ¿La app flashea en proceso? | **No** (SDLibrary1.dll solo lee versión + HID de pantalla/teclas) |
| ¿Imagen con firma/CRC interno? | **No** (verificado) |
| ¿El bootloader exige firma? | **Desconocido.** Por la referencia AK820 (sin firma, CRC), *probable que no* (INFERRED, confianza media) |
| Riesgo de brick por la ruta oficial (`APP`+`NEW`) | **Alto** (deja el flag `0x21436587` persistente y resetea esperando imagen) |
| Riesgo con SWD verificado | Bajo |

**Pruebas sin riesgo que quedan (ninguna ejecutada):**
1. Conseguir `UpDateToolV2\UpDateTool11-11.exe` (y, si aplica, `McuProgramIAPDll`) de soporte AJAZZ/Mirabox y
   analizarlo en estático: comandos HID, VID/PID de bootloader, bloque, CRC/firma. **PARADA 1** si exige firma.
2. Emulación "en seco" del bucle de teclas parcheado (`0x08006EF4`) con Unicorn/QEMU — sin dispositivo.
3. SWD **solo lectura**: volcar `0x08000000–0x0807FFFF`, desensamblar el bootloader y ver qué verifica.
4. Si el instalador debe abrirse, hacerlo con 7-Zip/innoextract/QtIFW en una máquina de análisis (no presente aquí).

**Modo "dry run"**: el AK820 expone `McuProgramIAP_SearchDev`/`GetDevInfo`/`GetProgress` (solo lectura/enumeración);
un UpDateTool equivalente podría tener un modo de solo lectura de identidad/versión. **No se ha ejecutado** y no
debe ejecutarse sin el dispositivo y una red de seguridad.

---

### Límites de este análisis
Sin ejecutar nada y sin el binario `UpDateTool11-11.exe`, la secuencia HID de flasheo del AKP153E y la verificación
del bootloader **no son reconstruibles**. El instalador de 377 MB no se pudo descomprimir (formato propio Qt, sin
7-Zip/innoextract en el equipo); solo se enumeró su ZIP de tienda embebido. Las inferencias sobre "sin firma" se
apoyan en la referencia AK820 (otro chip) y en la ausencia de firma en la imagen, no en el bootloader real.
