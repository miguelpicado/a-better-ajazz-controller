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

## Resultado 2026-11-16: parche v3 validado en hardware

- **v3 (pulsar/soltar) funciona** en el AKP153E real (`V2.293S.00.003`): cada pulsación llega como `state 1` y luego `state 0` (6 pares limpios con `orcadeck watch`, sin pérdidas ni duplicados); OrcaDeck con v3 funciona con normalidad. La unidad lleva v3 ahora. Imagen: `work/AKP153E_pressrelease_v3_NO_PROBADO.bin` (el nombre quedó; ya está probada), SHA-256 `dd2fa325216ff8e09ab90fecc4ffbe543b4b752016ba6bc51fe396ed816a0df5`.
- El parche de la tira (BAT) no hace falta con DRA y no se flashea.
- **Lección (causa de un falso negativo ese día):** el firmware solo reporta teclas tras el primer `STP` (flag `0x2000010c` en RAM, se pierde al reenchufar). Con el modo DRA, que no envía `STP`, tras reenchufar no llega ninguna tecla y la pantalla se apaga al pulsar; esto afectó por igual al original, a v1, a v3 y al parche identidad (`make_patch_identity.py`), y se confundió con un fallo del parche. Arreglado en el host (`STP` al abrir la sesión).
- Procedimiento de flasheo probado 5 veces ese día: ver `../../guide/USAGE.md` §4b. Si la herramienta se lanza sin su carpeta de trabajo o sin comillas en las rutas, se cierra en el 0 %.
- Siguiente: pulsación larga en el host (medir el tiempo entre `state 1` y `state 0`). No requiere más firmware.
