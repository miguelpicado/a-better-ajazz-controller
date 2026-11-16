# Parche de firmware pulsar / soltar para el AJAZZ AKP153E

El firmware de serie del AKP153E (`V2.293S.00.003`) envía **un** informe por pulsación, y solo al soltar la tecla, de modo que el software
no distingue pulsar de soltar (ni hay pulsación larga). Este parche hace que el firmware envíe un informe al **pulsar** (`state = 1`)
y otro al **soltar** (`state = 0`).

> **Úsalo bajo tu responsabilidad.** Flashear firmware puede dejar el dispositivo inservible. Probado en **una** unidad con firmware
> `V2.293S.00.003` (VID `0x0300`, PID `0x1010`). No llegó a bloquearse porque su bootloader (`EEEF:2929`) siempre acepta una imagen
> nueva, pero eso es comportamiento observado, no una garantía. Sin relación con AJAZZ ni respaldado por ella.

## Qué cambia

| | Firmware de serie | Parcheado (v3) |
|---|---|---|
| Informe al pulsar | no | `state = 1` |
| Informe al soltar | `state = 0` | `state = 0` (igual) |

ABAC detecta el firmware parcheado solo (`state 1` = pulsar, `0` = soltar) y con el de serie sigue tratando `0` como un clic.
**La app oficial de AJAZZ ignora `state`, así que con el parche cada acción se dispara dos veces**: usa ABAC (o tu propio código HID).

## Cómo funciona

El bucle de teclas llama desde seis sitios a dos funciones (el efecto «línea» y el envío del informe). El parcheador redirige esos seis `bl` a
un stub de 84 bytes colocado sobre 88 bytes de cadenas de error de libjpeg que no se usan (`0x0801AB9C..0x0801ABF3`). El stub de pulsar pone `TX[10] = 1`
y envía un informe extra, esperando (con límite) a que el endpoint USB esté libre; la ruta de soltar es la original. No hay checksum ni firma
dentro de la imagen. Detalles, emulación y riesgos: [`docs/research/firmware/`](../research/firmware/README.md).

## Requisitos

- Windows y Python 3 (solo biblioteca estándar).
- **Tu propia copia del firmware original**; aquí no se distribuye (copyright de AJAZZ). Con la app oficial instalada está en
  `%APPDATA%\HotSpot\StreamDock\update\AKP153E.bin` (96 496 bytes, SHA-256
  `15e03f5b5a593d665a382d500d8acf23b7e5aed87f64e4b7da450d1a36f54f6f`). El parcheador rechaza cualquier otro fichero. **Guarda una copia**: es tu vuelta atrás.
- El actualizador oficial `FirmwareUpgradeTool.exe` (viene con Stream Dock AJAZZ) y `abac.exe` (para `bootwatch`).

## Parchear

```powershell
cd docs\research\firmware\work
python make_patch_v3.py C:\ruta\AKP153E.bin AKP153E_pressrelease_v3.bin
```

El SHA-256 esperado de la salida es `dd2fa325216ff8e09ab90fecc4ffbe543b4b752016ba6bc51fe396ed816a0df5`; si el tuyo difiere, para.

## Flashear

1. Cierra la app oficial y ABAC (retienen el dispositivo).
2. `abac bootwatch 25`: envía la orden de actualización; la unidad reaparece como `EEEF:2929` (bootloader) y se queda ahí.
3. Lanza el actualizador **desde su carpeta y con las rutas entre comillas** (si no, se cierra en el 0 %):
   ```powershell
   $d = 'C:\Program Files (x86)\Stream Dock AJAZZ Global'
   Start-Process "$d\FirmwareUpgradeTool.exe" -WorkingDirectory $d -ArgumentList '768','4112','"C:\ruta\AKP153E_pressrelease_v3.bin"','"<un .png de esa carpeta>"'
   ```
   (`768` y `4112` son VID y PID en decimal.) Espera a «Updating successfully!!!».
4. **Desenchufa y vuelve a enchufar el USB.**

## Volver al firmware de serie

Repite los pasos 1-4 con el `AKP153E.bin` original. Si un flasheo se interrumpe, la unidad se queda en `EEEF:2929`: vuelve a ejecutar el actualizador.

## Importante: el firmware necesita `STP`

Tras cada reenchufe el firmware **no reporta ninguna tecla** hasta recibir su primer `STP` (un flag en RAM), y la pantalla se apaga al pulsar.
ABAC envía uno al abrir la sesión. Si escribes tu propio host, envía `STP` una vez tras el init y antes de pintar.
