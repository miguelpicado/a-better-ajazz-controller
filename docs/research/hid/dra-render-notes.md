# DRA render path (experimental) - what is implemented, what is untested

Opt-in: `ORCADECK_DRA=1` (default off; the BAT+STP path is untouched). Built from the hardware-confirmed facts in
`side-panel-lcd.md` §5.4. **Nothing here has been run on the device.**

## Implemented
- `proto.rs`: `dra_packets(n, Rect, jpeg)` (state 3 header + 512 B chunks; rejects w > 384, JPEG > 60 KB, empty),
  `bat_cell`, `centred_rect`, `dra_tile(key, tile, (dx,dy))` (tile centred on the BAT cell centre, clamped to 480x854),
  `parse_offsets`. Unit-tested (header bytes, rectangle math, clamps, limits).
- `render.rs`: `dra_tile_jpeg` (scale to tile px, rotate270 like `key_jpeg`, quality `ORCADECK_JPEG_Q`, steps down only
  if > 60 KB) and `dra_side_jpegs` (3 side cells at the BAT pitch 149, gaps filled by blending the facing edges, one
  continuous image, split into <= 384 px pieces; 3 cells = 378 px = 1 piece at panel x 49, y 12). Tests check SOF0
  = baseline, sampling 0x11 on all components (4:4:4), dimensions.
- `app.rs`: when enabled, `process_paints` draws ALL pending (= changed) keys in one pass, lowest key first, each
  `ORCADECK_DRA_GAP_MS` apart (default **50**, hardware: 15 tiles 50 ms apart all drew; any key 16..18 redraws the whole
  side strip from the current view), then the existing batch gap (`ORCADECK_GAP_MS`, 125) applies once. One changed key =
  one DRA (~60 ms); a full 15-tile repaint ~0.8 s. Unchanged keys are never pending, so never repainted. No BAT, no STP.
  The pass blocks the device loop (inputs queue in the HID buffer / reader thread). `ORCADECK_TRACE=1` logs
  `DRA pass: N draws in X ms (gap G ms)` per pass, next to the BAT `painted N keys ... in X ms` line, to compare both.
- `main.rs`: `dra` now builds its packets with `dra_packets` (state 3 only; state 4 / key != 0 removed); new
  `drapaint <x> <y> <w> <h> <r,g,b>` draws a solid rectangle to probe positions.

Env: `ORCADECK_DRA`, `ORCADECK_DRA_GAP_MS` (50), `ORCADECK_DRA_TILE_PX` (98, max 384), `ORCADECK_DRA_SIDE_PX` (80),
`ORCADECK_DRA_OFFSETS` (`key:dx,dy;...` panel px; key 16 = side strip offset).

## Untested assumptions (verify on hardware)
1. 98 px tiles centred on the BAT centre line up with the visible area; edge keys may need `ORCADECK_DRA_OFFSETS`.
   Tiles do not overlap (BAT pitch 148-149 > 98); the ~50 px gaps between them stay black.
2. 50 ms between DRA is safe for a whole pass (measured: 15 tiles drew; 3 at 200 ms also fine; not measured with a live
   clock/animation running or with side strip + tiles mixed); an ACK wait is NOT implemented (the read side lives in the input loop / reader thread;
   `Input::Ack` is not consumed). Lower the gap only with a measurement.
3. `image` crate JpegEncoder output is accepted by the firmware libjpeg (baseline 4:4:4 verified in tests, decode on
   device not).
4. Side strip: gap blending is cosmetic; cells use the 80 px BAT pitch/height; `ORCADECK_DRA_SIDE_PX` > 80 assumes the
   panel strip may grow in panel y (device horizontal), centred on y = 52.
5. `CLE ff` in `init` (reconnect) still runs and blacks the screen first, then the repaint pass draws all keys.
6. Presses during a DRA decode are assumed not lost (BAT needed gaps for that; DRA unknown).
7. Heartbeat and brightness (`LIG`) frames may arrive while a DRA decodes; untested.

## Per-key calibration (experimental, nothing run on hardware)
Visible window differs per key (edge vs centre keys), so each key has its own panel-pixel `dx, dy` (offset from the BAT
cell centre) and `w, h` (drawn size; w <= 384, clamped to the panel). Keys 16..18 are one strip: they share the slot of
key 16 (`dx, dy`, and `h` = strip size in panel y; `w` is ignored).
- **Store**: `%LOCALAPPDATA%\OrcaDeck\calibration.json` (`ORCADECK_CALIBRATION` = other path), written atomically (temp +
  rename): `{"enabled":bool,"keys":{"1":{"dx":0,"dy":0,"w":98,"h":98},...}}`. A key without entry uses
  `ORCADECK_DRA_TILE_PX` / `ORCADECK_DRA_OFFSETS` (fallback), else 98 px centred. Invalid file = ignored (logged), values
  clamped. `enabled: true` turns DRA mode on at startup (same as `ORCADECK_DRA=1`, which still forces it on).
- **Mode** (daemon, `calib.rs`): needs DRA mode on (refused otherwise: 409). While on, a physical press only SELECTS the key
  (no action runs; the key report arrives on release). The selected tile is drawn as a test pattern (1 px white border,
  diagonals, key number, red square top-left to see orientation); other keys keep content. Every change marks that key
  dirty; redraws go through the normal DRA pending/draw path, at most one pass per `ORCADECK_CALIB_GAP_MS` (default 150)
  and coalesced (only the latest value is drawn). Plugin paints for the selected key are dropped while it shows the
  pattern. Leaving the mode redraws the key with content and saves. Default path (calibration off) is unchanged.
- **API** (reuses the editor <-> daemon bridge `InspectorBridge`, so it only works inside `orcadeck run`; 501 otherwise):
  `GET /api/calibration` -> `{dra,on,selected,enabled,saved,keys:[{key,dx,dy,w,h,custom}]}`;
  `POST /api/calibration` with any of `on`, `enabled`, `save` (bool), `select` (1..18), `adjust`/`set` (`{key?,dx,dy,w,h}`,
  deltas / absolute), `reset` (`"all"` or key). Bad shape = 400, daemon refusal (mode off, DRA off) = 409.
- **Editor**: tab *Calibración* (Spanish like the rest of the editor): toggle, key grid (physical press selects too; polled
  every 0.7 s), arrows + keyboard arrows (Shift = 5 px), w/h buttons (+ lock), numeric fields, reset key / all, save indicator.

### Use on hardware
Start OrcaDeck normally with `ORCADECK_DRA=1` (scheduled task env or `orcadeck run`), `orcadeck edit` -> *Calibración* ->
tick *Calibración activa* -> press key 1 -> arrows until the white border is exactly the visible tile -> next key... -> untick
(saves). Tick *Usar DRA calibrado al arrancar* to keep DRA without the env var (restart needed; the DRA decision is made once).

### Untested assumptions to verify
1. Arrow direction: UI maps device right = panel -y, device down = panel +x (panel = device rotated 90 deg, from
   `rotate270`). Flip `CAL_ARROWS` in `daemon/crates/web/web/app.js` if wrong.
2. Test pattern orientation: drawn in device orientation and rotated like key images, so the number should read upright and
   the red square sit at the device top-left of the tile. Non-square sizes stretch the image to the rectangle.
3. 150 ms between calibration redraws is not lost; coalescing assumes redraws never overlap a decode.
4. Physical key press reaches the loop while DRA draws are running (release-only reports, no hold).
5. Side strip: selecting 16..18 shows the pattern in that cell only; edits move/resize the whole strip. `w` is ignored.
6. Scene switches / editor saves while calibrating repaint over the pattern (the key is repainted only on the next change).

## Side bar `panel_strip` (experimental, nothing run on hardware)
One builtin (`com.miguelpicado.orcadock.panel_strip`, `daemon/crates/builtin/src/strip.rs`) renders the whole side panel as one
device-oriented canvas (`h` x 378 px, text upright): big HH:MM, `VIE 13` / `NOV`, CPU / RAM / GPU bars with %, GPU TEMP and VRAM
rows, N/D when a metric is missing, orange/red from the `sys.rs` thresholds (red = block background). Preview:
`img/side-strip-preview.png` (decoded from the real DRA JPEGs; 80x378 x4 states, then 95x378 and 95x480).
- **Wiring**: SVG carries a `<!--strip WxH-->` marker; the builtin thread rasterises it at 2x, keeps it in `STRIP` (app.rs) and paints
  three 160 px slices as keys 16..18. DRA: `paint_dra_one` draws the whole strip from `STRIP` (`render::dra_strip_jpegs`, split in equal
  pieces <= 384 px, so 480 wide = 2 x 240); cells (5,1),(5,2) are ignored. BAT (DRA off): the slices are the cell images, using a
  cell-aligned layout (content never straddles the 69 px gaps). Scenes without the action are unchanged.
- **Size**: length fixed 378 (slot `w` ignored, as for the cell strip), across = slot-16 `h` (default `ORCADECK_DRA_SIDE_PX`=80), re-rendered
  on the next builtin tick (<= 2 s) after a calibration change; meanwhile the old image is resampled. While a side key is selected in
  calibration, the old cell test pattern is drawn instead.
- **Cadence**: sampling every 2 s (`Sys::poll`), clock shows minutes only, redraw only when the SVG text changes (new minute or new %).
  Never per second; one DRA (~60 ms) per change.
### Verify on hardware
1. `ORCADECK_DRA=1`, scene with the action at (5,0) only; set strip offset/height in the *Calibración* tab (select key 16..18; w is ignored).
2. Text orientation (HH:MM upright, `CPU` label left) and no mirroring; 3. legibility of the 11 px labels / micro rows; 4. nothing cut at the
visible edges (try h = 80, 88, 95); 5. no lost key presses with the 2 s redraws; 6. BAT mode (DRA off): three clean cells.
Open: font metrics are estimated (`em()`), real Segoe UI Bold may be a bit narrower; a 95+ px bar leaves the BAT cell geometry untouched.
