# Firmware

Investigación del firmware del AKP153E: viabilidad y emulación del parche de pulsar/soltar,
búsqueda de firmware V3, y análisis de las herramientas oficiales de actualización. Incluye
scripts y parcheadores en `work/` (**no probados**; los binarios de firmware no se publican, ver abajo). Regla dura: no
flashear ni ejecutar `FirmwareUpgradeTool.exe`.

## Binarios de firmware: solo en local

`backup/AKP153E.bin`, `work/*.bin` y `work/rwdata.bin` son firmware de AJAZZ o derivados directos. No se
publican: están en `.gitignore` y se conservan solo en el disco del autor. Los scripts de `work/`
(`make_patch*.py`, `emu/`) esperan encontrarlos en esas rutas; sin ellos solo se puede leer la documentación.
Si hace falta reproducir el análisis, vuelca el firmware de tu propia unidad.

## Decisión 2026-11-16: no se parchea el firmware

El usuario descartó flashear. Con `DRA` (teclas 98×98 y barra lateral continua, ver `../hid/side-panel-lcd.md` §5.4-5.5) el
resultado gráfico es el deseado, así que el parche de la tira (`AKP153E_sidestrip_NO_PROBADO.bin`) no hace falta. El parche
pulsar/soltar (v3) solo aportaría el cuadro amarillo de «tecla pulsada»; no se considera necesario (teclas rápidas, sin pulsaciones
perdidas, fps razonables). Todo el material de este directorio queda como investigación; la regla de no flashear sigue.
Ruta de recuperación probada, si algún día se retoma: `bootwatch` → `EEEF:2929` → `FirmwareUpgradeTool.exe` con `backup/AKP153E.bin`
(`updatetool-origin.md` §11).
