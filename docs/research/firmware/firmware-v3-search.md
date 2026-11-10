# Búsqueda de firmware V3 para Ajazz AKP153E (v1, PID 0x1010)

Fecha: 2026-11-10. Solo lectura: no se descargó ni ejecutó ningún binario, no se escribió al dispositivo y no se tocó la app Stream Dock. Se leyeron manifiestos JSON públicos (texto) y cadenas ASCII de ficheros locales ya instalados.

Niveles de confianza: ALTA = verificado en fichero/fuente primaria; MEDIA = inferencia razonada; BAJA = snippet de buscador o suposición.

## Resumen ejecutivo

1. No hay imagen de firmware V3 publicada para AKP153E / 293SV3 en los manifiestos del servidor de actualización. Los manifiestos solo listan MBox-N1 y VSD_M18 (confianza ALTA).
2. No hay evidencia de que un v1 (PID 0x1010) pueda pasar a V3. La propia app lo trata como un modelo distinto, y el firmware local más nuevo para tu unidad es V2.293S.00 (ALTA para lo local; MEDIA para la conclusión de hardware).
3. No hay documentación pública de flasheo, bootloader ni SWD para estos dispositivos, ni casos de brick documentados (BAJA, ausencia de resultados).
4. La alternativa barata es una unidad rev. 2 (PID 0x3010), con precios vistos entre 31 y 130 EUR/GBP según vendedor; no se puede garantizar la revisión antes de comprar (BAJA).

## 1. Servidor de actualización y firmware V3 descargable

### URLs halladas (cadenas dentro de `Stream Dock AJAZZ.exe`, en `C:\Program Files (x86)\Stream Dock AJAZZ Global\`; confianza ALTA)

- `https://cdn1.key123.vip/StreamDock/firmware/firmware-version-check.json` (el que usa el modelo "Stream Dock 293S" en la config embebida)
- `https://cdn1.key123.vip/Stock/firmware/firmware-version-check.json`
- `https://cdn1.key123.vip/Custom/Prajnasys/firmware/firmware-version-check.json`
- `https://cdn1.key123.vip/StreamDock/firmware/firmware-version-libcef.json`
- Versiones de la app: `https://cdn1.key123.vip/StreamDock/version/app-version-check-Windows.json` (y `_libcef`, macOS)
- Instalador: bucket Aliyun `hotspot-oss-bucket.oss-cn-shenzhen.aliyuncs.com` (BaiJia/..., GORIO/...), y `https://ajazz.key123.vip/`
- Mensaje de la app: "The current device firmware version does not support online upgrade" (clave `NotSupportDevice`). Es decir, no todos los firmwares permiten actualización online.

### Contenido de los manifiestos (WebFetch de JSON, 2026-11-10; ALTA)

- `StreamDock/firmware/firmware-version-check.json`: solo `MBox-N1` = `V3.N1.01.001` (`.../download/d13x_N1-HID_01.011.img`).
- `StreamDock/firmware/firmware-version-libcef.json`: `MBox-N1` `V3.N1.00.000` y `VSD_M18` `V3.VSDM18.02.015` (imágenes `d13x_*.img`, con md5).
- `Stock/firmware/firmware-version-check.json`: HTTP 404.
- No aparece ninguna entrada AKP153E, 293S ni 293SV3. Las imágenes `.img` de esos manifiestos son para otra familia (chip "d13x"), no para AKP153E.

### Firmware local ya presente (no descargado ahora)

- `%APPDATA%\HotSpot\StreamDock\update\AKP153E.bin`, 96 496 bytes, fecha 20-feb-2024. Contiene la cadena `V2.293S.00` y `HOTSPOTEK %s Stat!!!`; cabecera con tabla de vectores Cortex-M (base de flash 0x08000000, SP 0x200321C0). Es la imagen V2 de la línea 293S, coherente con el firmware de tu unidad `V2.293S.00.003` (ALTA). No es V3.
- En el mismo directorio está el instalador de la app (377 MB, 10-jun-2025).
- Logs de la app (`%APPDATA%\HotSpot\StreamDock\logs\`): líneas `SDSetting::updateFirmware` los días 2024-02-20, 22 y 26, 2024-03-25, 2024-04-30 y 2025-01-22, y en 2025-06-10 `Get firmware version : "V2.293S.00.003"`. Estas líneas indican que el botón de actualizar se activó varias veces; el resultado no queda registrado (los textos salen ilegibles).
- Herramienta auxiliar `FirmwareUpgradeTool.exe` (misma carpeta de instalación): tiene dos modos. Con ficheros `*.img` usa USB directo (`__cmd_upgrade_image2`, "fwc upgrade", FirmwareUpdater2) para la familia `d13x` (MBox-N1/VSD_M18); con 4 argumentos (`768 4112 <bin> <icono>`) flashea el AKP153E por HID. Ver `updatetool-origin.md`. Confianza ALTA.

Conclusión 1: el servidor público no ofrece V3 para AKP153E. Confianza ALTA para "no está en los manifiestos hoy"; no se descarta que exista fuera de ellos (canal "Custom" u OEM).

## 2. ¿Se puede pasar un v1 (0x1010) a V3?

Evidencia encontrada:

- `4ndv/opendeck-akp153` (`src/mappings.rs`): `AKP153E_PID = 0x1010` y `AKP153E_REV2_PID = 0x3010` son dispositivos distintos; README: "Ajazz AKP153E (rev. 2) (0300:3010)", además "AKP153R (rev. 2) (0300:3011)". HSV293SV3 usa 0x1014/0x1005 con VID de Mirabox (ALTA).
- `mirajazz` README: `protocol_version = 1` = "Mirabox 293S (not 293SV3) with latest firmware, Ajazz AKP153"; sin estado de pulsación (pulsar/soltar). `protocol_version = 3` = dispositivos con ambos estados; los "rev. 2" tienen PID que empieza por 3 (p. ej. AKP03 rev. 2 con PIDs 3xxx). Patrón repetido en el ecosistema: PID 1xxx = hardware/protocolo antiguo, 3xxx = rev. 2 con press/release (ALTA como patrón, MEDIA como causa).
- Aviso: "Mirabox 293S with latest firmware" sigue siendo pv1, o sea que ni siquiera el último firmware de la línea antigua da press/release.
- `ramtic/ajazz-pi` PROTOCOL.md: AKP153E `0300:3010` con `V3.AKP153E.02.009` emite 1 al pulsar y 0 al soltar (byte 10). Arranca en "demo mode" hasta enviar la secuencia de init (ALTA).
- Nombre de producto USB distinto entre V2/V3 según tu contexto: lo observado es que el cambio de PID acompaña al cambio de generación; la numeración del firmware (`V2.293S` frente a `V3.AKP153E.02`) también cambia de familia (nombre de modelo y revisión `02`).

Inferencia (MEDIA): lo más probable es que 0x1010 y 0x3010 sean placas/firmware de generaciones distintas (cadena de versión distinta, revisión de placa 02 explícita). Un cambio de PID lo puede hacer tanto un firmware como el hardware, así que no se puede afirmar que sea hardware distinto, pero ningún fabricante ofrece paso de V2 a V3 y la app lo marca como no soportado para online upgrade en firmwares viejos. No se encontró ningún hilo ni commit que cuente un upgrade v1 -> V3 (ausencia de evidencia, BAJA para "imposible", ALTA para "no documentado").

## 3. Documentación de flasheo, bootloader, SWD, MCU, bricks

- `Aiacos/ajazz-control-center`, `docs/architecture/FIRMWARE-UPDATES.md` y `FIRMWARE-UPDATES-IMPLEMENTATION.md` (clon en /tmp/ajz): decisión de no distribuir ni flashear firmware; para la familia Stream Dock (AKP03/05/153) solo dice "firmware en el ZIP del instalador, FirmwareUpdateTool"; no hay protocolo ni chip del AKP153E. Los chips documentados allí son Sonix SN32F2xx (teclados AK980/AK820), irrelevantes para este caso (ALTA).
- Esa misma documentación avisa que un corte de energía en pleno flasheo deja el dispositivo inservible y que el riesgo es mayor en dispositivos sin bootloader recuperable por hardware.
- Pista de MCU (MEDIA): la imagen `AKP153E.bin` tiene tabla de vectores ARM Cortex-M con flash en 0x08000000 y RAM en 0x20000000 (convención STM32 y clones: GD32, AT32, APM32, HK32). La cadena `HOTSPOTEK` apunta al fabricante OEM (Hotspot). El modelo exacto de MCU no se pudo determinar sin abrir la placa o analizar más a fondo el binario; no hay documentación pública de SWD/pines para este producto.
- Búsqueda web "bricked / bootloader / SWD HSV293S AKP153": sin resultados relevantes (BAJA = no hay casos publicados, que no equivale a que no existan).
- Los repos clonados (`mirajazz`, `opendeck-akp153`, `ajazz-pi`, `Uriziel01_Ajazz-AKP153-reverse-engineering`, `dvortsis_ajazz-companion-bridge`) documentan el protocolo HID, no el flasheo. El staff de Mirabox dice en su Discord que no dará detalles de protocolo (citado en el README de mirajazz).

## 4. Alternativa: AKP153E rev. 2 (PID 0x3010)

Precios vistos en snippets de buscador (no verificados en la página; BAJA):

- Klarna agregador: AKP153E desde unos 49,61 USD (envío gratis vía AliExpress) hasta 80,48 USD, fecha no indicada: https://www.klarna.com/us/shopping/pl/cl165/3472121949/Game-Controllers/Ajazz-AKP153E-Multifunctional-Console-Panel/
- HotUKDeals: oferta AKP153E/AKP03E a 30,69 GBP en la tienda oficial de Ajazz en AliExpress, vigencia sin confirmar: https://www.hotukdeals.com/deals/ajazz-akp153eakp03e-multifunctional-console-panel-with-code-5025026
- aio.lv (Letonia): 129,49 EUR negro/blanco, garantía 2 años (la fecha de validez del anuncio había caducado): https://aio.lv/en/product--ajazz-akp153e-b--12236196.md (la página devuelve 403 al fetch directo).
- WhatGeek lo lista sin stock: https://www.whatgeek.com/products/ajazz-akp153e-stream-deck-lcd-desk-helper
- No hay forma de saber desde el anuncio si el lote es rev. 2: ningún listado menciona "rev 2" ni el PID. Hay que preguntar al vendedor o aceptar el riesgo; el síntoma observable es PID 0x3010 y cadena `V3.AKP153E.02.xxx`. Hilo de un usuario con `0300:3010` en OpenDeck/Linux: https://www.answeroverflow.com/m/1445568738081640549 (BAJA).
- Tu unidad actual sigue siendo útil: la app original y mirajazz/opendeck la soportan con comportamiento "solo soltar".

## Fuentes y repos

- Servidor: https://cdn1.key123.vip/StreamDock/firmware/firmware-version-check.json y https://cdn1.key123.vip/StreamDock/firmware/firmware-version-libcef.json (leídos 2026-11-10).
- Repos locales: `mirajazz` (README, protocol_version), `4ndv_opendeck-akp153` (`src/mappings.rs`, README), `ramtic_ajazz-pi` (`docs/PROTOCOL.md`), `Aiacos/ajazz-control-center` (docs/architecture).
- Locales de la app: `C:\Program Files (x86)\Stream Dock AJAZZ Global\` y `%APPDATA%\HotSpot\StreamDock\{update,logs}`.

## Siguientes pasos sugeridos (no ejecutados)

1. Si quieres certeza sobre V3: preguntar a soporte de Ajazz (o su Discord) si existe un firmware V3 para PID 0x1010; es la única vía fiable.
2. Si necesitas press/release de verdad: comprar una unidad con PID 0x3010 confirmado por el vendedor.
3. Alternativa de software sin tocar firmware: emular press/release con temporizadores en el daemon (el evento al soltar ya llega).
