# Device Specifications & Support Table

OrcaDeck identifies supported stream deck hardware via declarative `DeviceSpec` records.
The active device determines:
- USB HID vendor ID (`vid`) and product ID (`pid`)
- Physical button geometry (`lcd_keys`, `side_keys`)
- Target render resolution (`key_px`, `side_px`)
- Image size ceiling (`max_jpeg`)
- Manifest identification (`manifest_id` matching `DeviceUUID` in `.sdProfile` scenes)
- Verification status (`verified`)

## Table of Specifications

| Name | Manifest ID (`DeviceUUID`) | VID | PID | LCD Keys | Side Keys | Key (px) | Side (px) | Max JPEG (B) | Verified | Source / Notes |
|---|---|---|---|---|---|---|---|---|---|---|
| `AKP153E` | `AKP153E` | `0x0300` | `0x1010` | 15 | 3 | 85 | 80 | 9000 | **Yes** | `docs/research/hid/hid-protocol.md` §0, §3.1, §4. Primary hardware target; verified on physical device. |
| `AKP153E-REV2` | `APK153E_293SV3` | `0x0300` | `0x3010` | 15 | 3 | 95 | 82 | 9000 | **No** | `docs/research/community/community-ideas.md` §1.2, §1.4, §7; `docs/research/hid/keystate-modes.md` §3. Rev 2 hardware / firmware v3. |
| `AKP153` | `AKP153` | `0x5548` | `0x6674` | 15 | 3 | 85 | 80 | 9000 | **No** | `docs/research/community/community-ideas.md` §1.2, §1.5. International variant / Mirabox 293S analyzed in Deckhand. |

## Activation & Safety Model

- **Default Device**: `AKP153E` (`verified: true`). The daemon defaults to the verified AKP153E and will only open devices matching this VID/PID unless explicitly overridden.
- **Unverified Device Opt-in**: Devices marked `verified: false` can **only** be selected when the user explicitly opts in via the environment variable `ORCADECK_DEVICE=<name>` (case-insensitive match on `name`, e.g. `ORCADECK_DEVICE=AKP153E-REV2`).
- **Prominent Warning**: When an unverified device is selected, `orcadeck run` prints a prominent warning at start:
  `warning: device <name> is not verified; expect wrong layouts; no firmware commands are ever sent`
- **Unknown Devices**: Devices with unknown VID/PID are never opened. Commands like `orcadeck probe` and `hidwatch` only inspect known specifications and enumerate read-only.
- **Firmware Safety**: OrcaDeck NEVER sends firmware update commands (`APP NEW`, bootloader triggers, or flashing commands). The HID protocol implementation is strictly limited to LCD canvas updates and input polling.

## Checklist for Adding / Verifying a Device

Before changing any device's `verified` flag from `false` to `true`, the following points must be tested and confirmed on real physical hardware:

1. **Output Report Payload Size ($N$)**:
   - Inspect the HID report descriptor (or run `orcadeck probe`).
   - Confirm whether the output payload length $N$ is 512, 1024, or another size.
2. **Input Report & Key Press Protocol**:
   - Run `orcadeck watch` with physical button presses.
   - Confirm whether the device sends single press-on-release reports (like AKP153E v1) or separate button-down (`state = 1`) and button-up (`state = 0`) reports (firmware v3 / rev 2).
3. **Key Layout & Matrix Indexing**:
   - Verify the grid mapping from physical (col, row) coordinates to hardware key IDs.
   - Confirm key index order (e.g. 1..=15 across 3 rows × 5 columns).
4. **Side Cell Characteristics**:
   - Confirm whether side cells are addressed as standard BAT keys (indices 16..=18) or as secondary display regions.
   - Verify that pixel dimensions (`side_px`) display without clipping, stretching, or white gutter lines.
5. **Key Pixel Dimensions & Rotation**:
   - Validate that `key_px` (e.g. 85px vs 95px) renders full-bleed without cropping.
   - Verify JPEG compression and rotation (270° CCW) match the physical screen orientation.
6. **Handshake & Keepalive**:
   - Check if the device emits `HANLDER` requests requiring a `HAN` reply.
   - Check if periodic `CONNECT` heartbeats (every 10s) prevent device sleep or disconnect.
7. **No Side Effects**:
   - Ensure the driver operates solely with display (`DIS`, `LIG`, `CLE`, `BAT`, `STP`) and handshake/heartbeat commands. Never issue commands outside documented display and input endpoints.
