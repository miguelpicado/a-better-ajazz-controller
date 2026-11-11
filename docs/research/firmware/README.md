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
