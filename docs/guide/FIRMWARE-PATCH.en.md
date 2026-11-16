# Press / release firmware patch for the AJAZZ AKP153E

The stock AKP153E firmware (`V2.293S.00.003`) sends **one** report per key press, and only after the key is released, so
software cannot tell a press from a release (no long press, no "held" state). This patch makes the firmware send a
report on **press** (`state = 1`) and another on **release** (`state = 0`).

> **Use at your own risk.** Flashing firmware can leave the device unusable. This was tested on **one** unit with firmware
> `V2.293S.00.003` (VID `0x0300`, PID `0x1010`). The device never bricked in testing because its bootloader
> (`EEEF:2929`) always accepts a new image, but that is observed behaviour, not a guarantee. Not affiliated with or endorsed by AJAZZ.

## What you get

| | Stock firmware | Patched (v3) |
|---|---|---|
| Report on press | no | `state = 1` |
| Report on release | `state = 0` | `state = 0` (unchanged) |
| Key LED/line effect | on press | on press (unchanged) |

OrcaDeck detects the patched firmware by itself (`state 1` = down, `0` = up) and keeps treating `0` alone as a click on stock firmware.
The **official AJAZZ app ignores `state`, so with the patch every action fires twice**: use OrcaDeck (or your own HID code).

## How it works

The key loop calls two helper functions (the "line on" effect and the report sender) from six places. The patcher redirects those
six `bl` instructions to an 84-byte stub placed over 88 bytes of dead libjpeg error strings (`0x0801AB9C..0x0801ABF3`). The press stub sets
`TX[10] = 1` and sends an extra report, waiting (bounded) for the USB endpoint to be free first; the release path is the original one.
No checksum or signature inside the image needs updating. Details, emulation and risks: [`docs/research/firmware/`](../research/firmware/README.md).

## Requirements

- Windows, Python 3 (the patcher uses only the standard library).
- **Your own copy of the original firmware.** It is not distributed here (AJAZZ copyright). With the official app installed it is
  `%APPDATA%\HotSpot\StreamDock\update\AKP153E.bin` (96 496 bytes, SHA-256
  `15e03f5b5a593d665a382d500d8acf23b7e5aed87f64e4b7da450d1a36f54f6f`). The patcher refuses any other file. **Keep a copy of it**: it is your way back.
- The official updater `FirmwareUpgradeTool.exe` (ships with Stream Dock AJAZZ).
- OrcaDeck (`orcadeck.exe`) for `bootwatch`.

## Patch

```powershell
cd docs\research\firmware\work
python make_patch_v3.py C:\path\to\AKP153E.bin AKP153E_pressrelease_v3.bin
# prints sha256 dd2fa325216ff8e09ab90fecc4ffbe543b4b752016ba6bc51fe396ed816a0df5 len 96496
```

The expected SHA-256 of the output is `dd2fa325216ff8e09ab90fecc4ffbe543b4b752016ba6bc51fe396ed816a0df5`; if yours differs, stop.

## Flash

1. Close the official app and OrcaDeck (they hold the device).
2. `orcadeck bootwatch 25` — sends the update command; the device re-enumerates as `EEEF:2929` (the bootloader) and stays there.
3. Run the updater **from its own folder, with quoted paths** (otherwise it closes at 0 %):
   ```powershell
   $d = 'C:\Program Files (x86)\Stream Dock AJAZZ Global'
   Start-Process "$d\FirmwareUpgradeTool.exe" -WorkingDirectory $d -ArgumentList '768','4112','"C:\path\AKP153E_pressrelease_v3.bin"','"<any .png from that folder>"'
   ```
   (`768` and `4112` are the VID and PID in decimal.) Wait for «Updating successfully!!!».
4. **Unplug and replug the USB cable.**

## Go back to stock

Repeat steps 1-4 with the original `AKP153E.bin`. If a flash is interrupted the device stays in `EEEF:2929`; run the updater again.

## Important: the firmware needs `STP`

After every replug the firmware reports **no keys** until it receives its first `STP` command (a RAM flag), and the screen blanks on a key
press. OrcaDeck sends one when it opens the session. If you write your own host, send `STP` once after init, before drawing.
