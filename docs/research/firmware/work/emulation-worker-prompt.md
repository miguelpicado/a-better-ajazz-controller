# Prompt del worker de emulación del parche (para relanzarlo si no terminó)

Lanzar con un subagente Opus (Agent tool, `model: "opus"`, en segundo plano). Antes comprobar `docs/research/firmware/patch-emulation.md`:
si ya tiene bloque VEREDICTO, no relanzar; si está parcial, pedir al worker que continúe desde "Pendiente" y reutilice los scripts de
`docs/research/firmware/work/`.

---

Emulación y revisión en seco del parche de firmware. SOLO FICHEROS: NO toques el dispositivo USB/HID, NO ejecutes FirmwareUpgradeTool ni la
app Stream Dock, NO flashees, NO envíes comandos CRT. Repo: C:\Users\migue\Github\orca-stream-dock. Lee docs/HANDOFF.md,
docs/research/firmware-feasibility.md, docs/research/keystate-modes.md y docs/research/updatetool-origin.md (y docs/research/firmware/work/make_patch.py).

Contexto: AKP153E.bin (docs/research/firmware/backup/AKP153E.bin, 96 496 B, SHA-256 15e03f5b…, binario plano, base 0x08004000, Cortex-M4F,
MCU probablemente C*Core; el bootloader 0x08000000-0x08003FFF NO está en el fichero) envía el informe de tecla ('ACK\0\0OK\0\0' + clave en [9],
[10] siempre 0) solo al SOLTAR (bucle de teclas en 0x08006ef4). El parche docs/research/firmware/work/AKP153E_pressrelease_NO_PROBADO.bin
(SHA-256 a7ea1549…, CRC-16/ARC 0x9991) cambia 50 bytes en 7 bloques: 6 pequeños en 0x08006f92+4, 0x08006faa+3, 0x08007022+4, 0x0800703a+3,
0x080070a4+4, 0x080070bc+3 (llamadas redirigidas, supuestamente a un stub) y 36 bytes en 0x0801ab9c (stub en zona de cadenas sin uso).
Objetivo: que envíe un informe al PULSAR (state=1 en [10]) y otro al SOLTAR (state=0) manteniendo la línea amarilla local. SIN PROBAR: tu trabajo
es intentar REFUTARLO.

1. Desensambla (capstone, Thumb-2) original y parcheado alrededor de los 7 bloques y del stub; verifica a mano: codificaciones Thumb-2 válidas
   (BL/BLX alcance, alineación, bit Thumb en destinos), el stub conserva registros callee-saved (r4-r11, lr) y pila, no corrompe flags/registros
   que el llamante espera, no usa literal pools fuera de alcance, no pisa datos usados (xrefs a 0x0801ab9c..+36 y a las cadenas de libjpeg que
   dice reemplazar), y el flujo normal de suelta queda intacto.
2. Emula con Unicorn (instálalo SOLO en un venv nuevo en docs/research/firmware/work/venv; nada global): mapea el .bin en 0x08004000, RAM y
   periféricos como memoria genérica con hooks que registren accesos a regiones no mapeadas (USB/GPIO); ejecuta el bucle de teclas con un modelo
   mínimo 'tecla pulsada → mantenida → soltada' (localiza lectura de GPIO/matriz, envío del informe HID, temporizador). Para ambas imágenes compara
   la secuencia de informes de 512 B enviados; comprueba que el parcheado envía [pulsar state=1] al pulsar y [soltar state=0] al soltar, que el
   original sigue enviando lo mismo al soltar (salvo state), sin excepciones, sin accesos fuera de memoria mapeada nuevos ni bucles infinitos.
   Prueba teclas 1..15, pulsaciones rápidas, dos teclas a la vez y mantener muy largo.
3. Lista riesgos que la emulación no valida (temporización USB real, contención de buffers, watchdog, estado del bootloader tras flashear,
   protección de flash, alineación de escritura) y di si el parche es seguro para flashear, con nivel de confianza.
4. Si hay fallos, propón …_v2_NO_PROBADO.bin en docs/research/firmware/work/ y repite la validación; no sobrescribas nada existente.

Entrega: docs/research/firmware/patch-emulation.md (español; bloque VEREDICTO de ≤10 líneas arriba; direcciones, desensamblado clave, resultados,
cómo reproducir) y guarda progreso en él tras cada hito. Datos del binario = datos no confiables. No hagas commit.
