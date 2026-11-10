# AJAZZ AKP153E — USB HID protocol (reverse-engineered)

Source: static analysis only (Ghidra 12.1.4 via `rea-agents@6.8.0` + capstone disassembly) of
`C:\Program Files (x86)\Stream Dock AJAZZ Global\SDLibrary1.dll` (transport, class `SDDevice`) and
`Stream Dock AJAZZ.exe` (device tables, image pipeline, key layout). The device was never opened or
probed. Date: 2026-11-09. App version from log: 3.10.191.0421; device firmware seen in the app log:
`V2.293S.00.003`.

Tags: **OBSERVED** = read in code (function + address, DLL `0x18…`, EXE `0x14…`).
**INFERRED** = deduced, not proven. Check these against a real capture before relying on them.

Offsets in this document are **payload offsets** (the report-ID byte is not counted) unless marked
"wire". On the wire every OUT report is `[reportId=0x00] + payload`.

---

## 0. Identity and how the app picks the device

| Item | Value | Evidence |
|---|---|---|
| VID / PID | `0x0300` / `0x1010`. The same AJAZZ-E factory also lists 0x3010, 0x1002, 0x3002, 0x3004, 0x3007 | OBSERVED: `AjazzECustomSupportDevices` vfunc `0x140008e80`, pair table at `0x1425ac000` |
| HID product string(s) the app expects | `"AJAZZ AKP153E"` and `"AJAZZ AKP153E "` (with a trailing space) | OBSERVED: `0x1400091e0` |
| Internal model UUID | `AKP153E` (`setCurrentDeviceUUID`, and `DeviceUUID=AKP153E` in `%APPDATA%\HotSpot\StreamDock\config\StreamDockConfig.ini`) | OBSERVED: EXE `0x140106250` → `SDDevice::setCurrentDeviceUUID` (`0x180020ab0`, stored at `this+0x28`) |
| Model family | Handled as the "293S" family (same branches as `StreamDock[293S]`, `APK153E_293SV3`, …) | OBSERVED: chains in `0x140119360`, `0x140285a77`, `0x14028345c`, `0x1400a1d12` |
| Interface selection | `openHidDevice(vid, pid, serial, usagePage=-1, usage=-1, reportId=0)`. For AKP153E it opens the **first** hidapi enumeration entry that matches VID/PID/serial (no usage filter) | OBSERVED: EXE `0x14023c07c..0x14023c0bd`. DLL `SDDevice::openHidDevice` `0x18001b760` (filters on `usage_page`/`usage` only when they are not -1). Other models use usagePage `0xFFA0`/usage 1 |
| Report ID | `0x00` (`this+0x98`) | OBSERVED: same call site |

If the device exposes more than one HID interface, prefer the vendor-defined one (usage page ≥ 0xFF00). That is **INFERRED**: the app relies on enumeration order.

## 1. Transport, report sizes, report types

* **OUT data = HID output reports via `hid_write`** (interrupt OUT / `WriteFile`). Feature reports are not used for AKP153E. OBSERVED: `SDDevice::writeData` `0x180021490` calls `hid_write(h, buf, maxPacket+1)`. `sendHidFeatureReport` (`0x18001e920`) is never called by the EXE.
* **Packet size** `maxPacket` (`this+0x30`) = `hid_get_output_report_lenght()`. That vendor hidapi export returns `OutputReportByteLength - 1`, so the report-ID byte is not counted. OBSERVED: `0x180139560` (`movzx eax,[rcx+0xc]; dec eax`). It falls back to 64 if the value is ≤ 0. `setMaxPacketSubtractLenght` is only set to 1 for K1Pro and stays 0 for AKP153E (OBSERVED `0x140106250`).
  * The concrete number for AKP153E is **unknown** because it comes from the device's HID caps. Read it from the report descriptor or `HidP_GetCaps`. It is probably 512 or 1024 (**INFERRED**).
  * Every write is exactly `1 + maxPacket` bytes: `[0x00][payload zero-padded to maxPacket]`.
* **IN** = `hid_read_timeout(h, buf, inputLen+1, 3000 ms)` in a dedicated thread. Before each read, `buf[0]` is pre-set to the report ID. Parsing starts at `buf[0]` = first payload byte, because Windows hidapi strips report ID 0. OBSERVED: `SDDevice::readDataFromHidDevice` `0x18001c5d0`.
* **Firmware version** = HID **GET_REPORT(Input, id 0)** of `inputLen+1` bytes. The ASCII string starts at `buf[1]`, e.g. `V2.293S.00.003`. It is read once in `openHidDevice` when `supportGetDeviceVersion` is set, and AKP153E is in that list. OBSERVED: `0x18001b760` (input-report branch), `SDDevice::getHardwareFirmwareVersion` `0x180018ea0`, EXE list `0x14028e170`, and the app log `Get firmware version : "V2.293S.00.003"`.

## 2. Host → device commands

Common prefix: payload `[0..2] = 'C','R','T'`, `[3..4] = 0x00 0x00`, then a 3–7 byte ASCII verb at `[5..]`. Every byte not listed below is 0x00. Queued `add*` commands are built without the report ID, and the write thread prepends it. Immediate `send*` commands build the wire form directly. Both produce identical bytes on the wire.

| Command | Payload layout (hex offsets) | Function / addr | Notes |
|---|---|---|---|
| Wake / display on `DIS` | `CRT\0\0DIS` ; `[0x0a]=0x01` | `addWakeUpScreenPack` `0x180017020`, `sendWakeUpScreenPack` `0x1800208d0` | At write-thread start the lib queues `CRT\0\0DIS` with `[0x0a]=1` only if the software UUID is "LMA", otherwise `[0x0a]=0` (`writeDataToHidDevice` `0x180021820`). OBSERVED |
| Screen off `HAN` | `CRT\0\0HAN` ; `[0x0a]=0x01` | `addScreenOffPack` `0x180016e20`, `sendScreenOffPack` `0x18001f3a0` | Sent by the screen-off timer (`timerEvent` `0x180021370`, action 1) and in `~SDDevice` (`0x180013d60`). OBSERVED |
| Brightness `LIG` | `CRT\0\0LIG` ; `[0x0a]=b`, where b is clamped to 0..100 | `addAdjustBrightnessPack` `0x180014a10` | Sent right after the write thread starts, using the value saved in the ini under `[DeviceBrightness]` (AKP153E is in the brightness-capable list `0x14028ccf0`). OBSERVED |
| Clear key `CLE` | `CRT\0\0CLE` ; `[0x0b]=key` (1..18, see §4) | `addClearCommand` `0x180014df0`, `sendClearCommand` `0x18001dc00` | OBSERVED |
| Clear all | `CRT\0\0CLE` ; `[0x0b]=0xFF` | `addClearAllCommand` `0x180014c40` | OBSERVED |
| Disconnect / show logo | `CRT\0\0CLE` ; `[0x0a]='D'`, `[0x0b]='C'` | `sendDisconnectCommand` `0x18001ddd0` | Sent on shutdown, before HAN. OBSERVED |
| Flush / refresh `STP` | `CRT\0\0STP` | `addFinishCommand` `0x180014fb0`, `getFinishCommand` `0x180018b30` | The app appends it after a batch of images (`getFinishCommand` + `appendSendData(list)`, e.g. EXE `0x14008cf70/0x14008cfb9`). Page byte `[0x09]=0x21/0x22/0x23` applies only to `StreamDock[321D]`/`H3`/`StreamDock[0108D]`. OBSERVED |
| Image header `BAT` | `CRT\0\0BAT` ; `[0x0a]=len>>8`, `[0x0b]=len&0xFF`, `[0x0c]=key` | `sendPicSizeCommand` `0x18001ee10` | The length is **16-bit big-endian** byte count of the JPEG. See §3. OBSERVED |
| Heartbeat `CONNECT` | `CRT\0\0CONNECT` (`[5..0x0b]`) | `addHeartbeatPack` `0x1800153b0`, `timerEvent` `0x180021370` | Every 10 s (`startTimer(10000)` in `openHidDevice`). Skipped when UUID = `StreamDock[296]`. Heartbeat is enabled by the EXE except for VID 0x5548/PID 0x6667 (EXE `0x14011f2c0`). OBSERVED |
| Handshake reply `HAN` | payload `[0..2]='H','A','N'` (**no CRT prefix**) | `sendHandshakePack` `0x18001e5d0` | Sent only as a reply to device `HANLDER` (§5), and only while the write thread is idle. Not sent for `StreamDock[296]`. OBSERVED |
| Get version `VER` | `CRT\0\0VER` | `sendGetHardwareFirmwareVersion` `0x18001e430` | Only for old `StreamDock[293]` (serial ≠ 3FFFEFD81FFF), never for AKP153E. OBSERVED |
| Mode `MOD` | `CRT\0\0MOD` ; `[0x0a]=mode` | `addModeChangedCommand` `0x180016930` | Not used for AKP153E as far as seen. OBSERVED layout |
| Orientation `SET` | `CRT\0\0SET` ; `[0x0a]=a`, `[0x0b]=b` | `sendChangeDirectionCommand` `0x18001d850` | Purpose unclear (INFERRED: rotation, on other models) |
| Region image `DRA` | 32-byte header `CRT\0\0DRA`; `[8..0x0b]=len+32` (BE32), `[0x0c]=key`, `[0x0d..0x0e]=w`, `[0x0f..0x10]=h`, `[0x11..0x12]=x`, `[0x13..0x14]=y` (BE16), then the JPEG | `getSecondaryScreenPicInfo` `0x180019ae0` | ImageStruct types 0x10/0x12/0x13. Used for "SecondaryScreen" controllers (MOLA/KB8IN1/N-series). **Not used by AKP153E**: its side cells are ordinary BAT keys. OBSERVED |
| Logo `LOG`, GIF `LOG…` | `CRT\0\0LOG` + BE32 len + key | `sendLogoSizeCommand` `0x18001ea40`, `sendGIFToJPGInfo` `0x18001e000` | Boot-logo / screensaver upload. Out of scope |
| `QUCMD`, `KEY`, lamp | — | `addQUCMDCommand` `0x180016b20`, `addKeyboard*` `0x180015670`, `addLampControlCommand` | For keyboard and lamp models. Not used by AKP153E |

## 3. Image upload

### 3.1 Image format (EXE)
* **Key image size: 85 × 85 px. Side-panel "Information" cells: 80 × 80 px.** OBSERVED: `FUN_140281930` (target size per UUID and controller type). For AKP153E the `0x14028345c` group is taken: controller type `"Information"` → `QSize(0x50,0x50)` at `0x140283632`, otherwise `QSize(0x55,0x55)` at `0x14028360c`. In the EXE UI, `"Keypad"` = the 15 LCD keys and `"Information"` = the 3 side cells (EXE `0x140119360`).
* **Encoding: JPEG**, built with `DataFormatConversion::QImageToQByteArray(img, "JPG", quality, rotate=true, angle=270.0, mirrorH=false, mirrorV=false)`. OBSERVED: EXE `FUN_140284540`. AKP153E is in the first device list, which jumps to `0x140285a77`; it is not in the second list, which ends at `0x140285d04/0x140285d0f` with `r14b=1` and `esi=0x10e`. Then `0x140286157..0x1402861c3`. DLL implementation: `0x180003090`.
  * Rotation is `QTransform::rotate(270, Qt::ZAxis)` followed by `QImage::transformed(…, SmoothTransformation)`. Qt uses a y-down axis, so this is **270° clockwise = 90° counter-clockwise** on screen (INFERRED from Qt semantics). The JPEG is not mirrored.
  * Quality starts at 100. While `jpeg.size > limit` the app lowers quality by 4 and re-encodes; once quality would go below 0 it uses quality -1 (Qt default). The limit is **9000 bytes**, or **3000** if the widget has a secondary background image (`this+0x338` non-null). OBSERVED: `0x1402861ea..0x1402862a7`, limits in `FUN_14006fbb0`/`FUN_140047e90`.
* Each image is one `ImageStruct` (0x60 bytes): `+0x00 type=5`, `+0x2c key`, `+0x30 QByteArray jpeg`, plus a list of timeouts at `+0x08..+0x28` (3000, 20000, 300000, 20000, 0x500000, 10000, 30000, 3000, 5000 ms/bytes). OBSERVED: `FUN_140037480`, `FUN_14006fbb0`.

### 3.2 Wire sequence for one key (DLL write thread, `writeDataToHidDevice` `0x180021820`, type 5)
1. `BAT` packet (`sendPicSizeCommand`), with `key` = hardware index from §4.
2. For `XF-A3501`, `StreamDock[295]` and old `StreamDock[293]` only, the thread waits up to 500 ms for an ACK. **AKP153E does not wait.** OBSERVED.
3. JPEG bytes in consecutive chunks of `maxPacket` bytes. Each chunk is sent as `[0x00][chunk]`, and the last chunk is zero-padded to `maxPacket`. There is no sequence number and no per-chunk header. OBSERVED: chunk loop after `LAB_1800240b8`.
4. The thread waits up to **1000 ms** for the device ACK (§5), which wakes the writer. It then takes the next queued item. OBSERVED, though the decompiled control flow here is tangled; treat the exact wait as high-confidence INFERRED.
5. After the batch the app queues `STP` (type 0xD), which goes through the same path (one packet, then a wait for ACK). OBSERVED that STP follows image lists; INFERRED that it is required to latch the images.

Other queue item types: 0x0E sleeps 100 ms after sending, and 0x0F is a plain command (heartbeat, LIG, …). Every queued item is sent with the same chunk logic.

## 4. Key index mapping (AKP153E)

The app gives every UI cell a hardware index with `setKeyIndex(widget, n)` (`0x14008fda0`: `mov [rcx+0x1d0], edx`). Images are addressed by that index (`ImageStruct+0x2c` ← `widget+0x1d0`, `FUN_14006fbb0`). Incoming key events are matched against the same index (`0x1400a8d7c..0x1400a8dbb`: loop until `widget.keyIndex() == key`, getter `0x140061660`). Layout builder for AKP153E: EXE `0x140119360` (reached from `0x1401101e9`). Keypad index = `FUN_140297130(uuid, i)`, a jump table at RVA `0x298858`, with `i = row*5 + col`.

UI grid: 3 rows × 5 columns, plus a right-hand column of 3 side cells (profile coordinates `col,row`: keys `0..4,0..2`, side cells `5,0..2`).

| | col 0 | col 1 | col 2 | col 3 | col 4 | side panel (col 5) |
|---|---|---|---|---|---|---|
| row 0 (top) | **13** | **10** | **7** | **4** | **1** | **16** (top cell) |
| row 1 | **14** | **11** | **8** | **5** | **2** | **17** |
| row 2 (bottom) | **15** | **12** | **9** | **6** | **3** | **18** (bottom cell) |

OBSERVED: keypad values from the jump table. Side cells come from the loop at `0x140119587..0x14011964f`: indices 16, 17, 18 are added top→bottom to a `QVBoxLayout`. The side cells take 80×80 images through the same BAT command. The app also treats them as press targets (`installEventFilter` and the same key lookup), so they may report events even though they have no physical button. That last part is INFERRED.

## 5. Device → host input reports

Parser in `readDataFromHidDevice` `0x18001c5d0`. Offsets are into the read buffer, which starts at payload byte 0.

| Pattern | Meaning | Evidence |
|---|---|---|
| `48 41 4E 4C 44 45 52` = `"HANLDER"` (sic) at `[0..6]` | Device handshake request → host replies `HAN` (§2) if the write thread is idle | OBSERVED `0x18001c9bd..0x18001ca0e` |
| `"error"` at `[0..4]` | Logged only | OBSERVED |
| `[0..2]="ACK"`, `[5..6]="OK"`, `[9]=0x00` | Command/image ACK → `QWaitCondition::wakeAll`, which releases the writer wait in §3.2 | OBSERVED `0x18001ceec..0x18001cf44` → `0x18001cb16` |
| `[0..2]="ACK"`, `[5..6]="TF"` or `"CB"`…`[9..10]="OK"` | Storage free-space reply for logo uploads (value from bytes 9..13). Not used for AKP153E | OBSERVED |
| `[0..2]="CRT"`, `[5..7]="MOD"` | Mode report, `[0x0a]` = mode | OBSERVED |
| anything else | **Key event: key = `[9]`, state = `[10]`** → `sigHandleKeyEvents(key, state)` | OBSERVED `0x18001d0d7` → `0x18001cc1b` |

* AKP153E is not in the `StreamDock[293]`/`XF-A3501`/`StreamDock[295]` group, so the DLL applies **no 200 ms debounce** and forwards every event at once. OBSERVED: `0x18001d08e..0x18001d0e0`. The key event most likely arrives as `"ACK\0\0OK\0\0" key state` (INFERRED: same prefix as the ACK but with `[9]` ≠ 0).
* If the device is in software sleep (`this[0x15c]`), any key report only wakes it (`sigOnDeviceWakeUp`) and does not trigger an action. OBSERVED `0x18001cf49`.
* **State byte**: for the 293S family, including AKP153E, the EXE ignores `state` and fires the action (press) on every report (EXE `0x1400a8d69..0x1400a8eb1`, `FUN_14007d690(widget, true, 50, "")`). Other models use 0 = release and 1 = press (`0x1400a2fe2`). OBSERVED. INFERRED: AKP153E firmware V2 probably sends one report per press, or the app simply does not use releases. A replacement driver should handle both 0x01 and 0x00 and confirm with a capture.

## 6. Handshake, heartbeat, timing, error rules

* **Open**: `hid_open_path`, read the strings, read the firmware via GET_REPORT, start a 10 s heartbeat timer, then start the read and write threads (`openHidDevice` `0x18001b760`). OBSERVED.
* **Write-thread start** (`0x180021820`): clear the queue, queue `DIS`, then `LIG` with the saved brightness. After that the EXE pushes all key images and an `STP` (§3). OBSERVED for DIS and LIG. INFERRED for the exact order of the EXE image push.
* **Handshake**: device-initiated only (`HANLDER` → `HAN`). The host never sends HAN first on AKP153E. OBSERVED.
* **Heartbeat**: `CRT\0\0CONNECT` every 10 000 ms. OBSERVED.
* **ACK waits**: up to 1000 ms after each queued item; no 500 ms pre-wait after BAT on AKP153E. OBSERVED/INFERRED (see §3.2).
* **Read**: blocking 3000 ms timeout per loop. When `hid_read` returns -1, the error counter is incremented and the loop sleeps 50 ms. OBSERVED.
* **Write errors**: when `hid_write` returns -1 with `0x000003E5` (ERROR_IO_PENDING), the app emits `sigErrorIOPending` and resets the counter. Any other error increments the counter, and after more than 4 consecutive errors the device is closed (`0x0000048F`/`0x00000005` decide the close reason). OBSERVED `SDDevice::writeData` `0x180021490`.
* **Queue limit**: once queued bytes exceed `setMaxDataSize`, the app emits `sigTooMuchDataWaitingToBeSent` and drops the data. OBSERVED.
* **Shutdown** (`~SDDevice` `0x180013d60`): `sendTransparentBackground` (only for `StreamDock[296]`), then `CLE…DC`, then `HAN` screen-off. OBSERVED.

## 7. AKP153E vs other models in the library

| Aspect | AKP153E | Others (examples) |
|---|---|---|
| Interface filter | usagePage/usage = -1, reportId 0 | most models: usagePage 0xFFA0, usage 1; some reportId 4 (`0x14023c08b`) |
| Key layout | 15 keys + 3 side cells, indices per §4 (16 - f(i) is **not** applied) | KB8IN1-1 layout uses `16 - f(i)` (`0x140110326..0x140110335`) |
| Image size | 85×85 keys, 80×80 "Information" | 293/295: settings-dependent 100/126; N4: 112 (176×112 knob strip); N1: 36; M18: 64, … (`FUN_140281930`) |
| Rotation | 270° (QTransform), no mirror | KB8IN1-1 90°, many 180° or mirror H+V, others 0 (`FUN_140284540`) |
| BAT pre-ACK wait | none | 500 ms for XF-A3501 / StreamDock[295] / old StreamDock[293] |
| Key debounce in DLL | none | 200 ms for StreamDock[293] / XF-A3501 / StreamDock[295] |
| Key state | ignored (press on every report) | 0/1 release/press on other families |
| HAN handshake reply / heartbeat | yes | skipped for `StreamDock[296]` |
| Firmware query | GET_REPORT input (startup) | old 293: `CRT VER` polling |
| Side screen | plain BAT keys 16–18 | MOLA/KB8IN1/N-series "SecondaryScreen" use `DRA` with x/y/w/h |

## 8. Minimal driver pseudocode (Node-style)

```js
// INFERRED glue; protocol bytes per §2–§5.
const dev = openHid({ vendorId: 0x0300, productId: 0x1010 /* prefer vendor usage page */ });
const N = outputReportLength(dev) - 1;      // e.g. 512 or 1024 — read from HID caps, never hardcode blindly
const crt = (verb, extra = {}) => { const p = Buffer.alloc(N); p.write('CRT', 0); p.write(verb, 5);
  for (const [o, v] of Object.entries(extra)) p[+o] = v; return p; };
const send = (payload) => dev.write([0x00, ...payload]);            // 1 + N bytes, report id 0

let ackWaiter = null;                                                // resolved by reader on ACK
const waitAck = (ms = 1000) => new Promise(r => { ackWaiter = r; setTimeout(r, ms); });

// connect
send(crt('DIS'));                               // wake (optionally [0x0a]=1)
send(crt('LIG', { 0x0a: 80 }));                 // brightness 0..100
send(crt('CLE', { 0x0b: 0xff })); await waitAck();
setInterval(() => send(crt('CONNECT')), 10_000);

// set key image (key = 1..18 per §4)
async function setKey(key, img) {
  const side = key >= 16, px = side ? 80 : 85;
  let q = 100, jpg;
  do { jpg = jpeg(rotate(resize(img, px, px), 270 /* Qt CW = 90° CCW */), q); q -= 4; } while (jpg.length > 9000 && q >= 0);
  send(crt('BAT', { 0x0a: jpg.length >> 8, 0x0b: jpg.length & 0xff, 0x0c: key }));
  for (let o = 0; o < jpg.length; o += N) { const c = Buffer.alloc(N); jpg.copy(c, 0, o, o + N); send(c); }
  await waitAck();
}
async function commit() { send(crt('STP')); await waitAck(); }

// read key events
dev.on('data', (d) => {                         // d[0] = first payload byte (report id stripped)
  if (d.toString('latin1', 0, 7) === 'HANLDER') return send(Buffer.concat([Buffer.from('HAN'), Buffer.alloc(N - 3)]));
  const ack = d.toString('latin1', 0, 3) === 'ACK' && d.toString('latin1', 5, 7) === 'OK';
  if (ack && d[9] === 0) return ackWaiter?.();
  if (d.toString('latin1', 0, 3) === 'CRT') return;            // MOD etc.
  onKey(d[9], d[10]);                            // key 1..18, state (app ignores it for AKP153E)
});
// shutdown: send(crt('CLE', {0x0a:0x44, 0x0b:0x43})); send(crt('HAN', {0x0a:1}));
```

## 9. Open points to verify with a USB capture (not done: no device access in this task)
1. The exact `OutputReportByteLength`/`InputReportByteLength`, which give `N`.
2. Whether the key-event report really carries the `ACK\0\0OK\0\0` prefix, and the values of the state byte (press-only, or press/release).
3. Whether the side cells (16–18) emit events.
4. On-screen direction of the 270° rotation (Qt CW ⇒ visually 90° CCW).
5. Whether `STP` is mandatory after each image or only once per batch.

## Artifacts
Decompiled pseudocode (scratch, not committed): `…\scratchpad\dec` (SDDevice), `dec_gen` (SDGeneral*), `dec_ctl` (SDController), `dec_dfc` (DataFormatConversion), `dec_helpers` (DLL `0x180039000-0x18003c200`), `dec_exe1/2/3` (EXE ranges `0x1402001a0-0x14020f085`, `0x140037000-0x140072000`, `0x140106000-0x140122000`). The helper scripts are in `…\scratchpad\tools` (`dis.py`, `bytes.py`, `xrefnp.py`, `callers.py`, `fn.py`).
