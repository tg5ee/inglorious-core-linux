# Implementation notes

## Scope and decisions

Implemented as a single crate, with pure command bodies/parsers in `protocol.rs`,
all HID and filesystem I/O in `device.rs`, serde state and batch sequencing in
`state.rs`, numeric mappings in `tables.rs`, and eframe controls in `src/ui/`.
Dependencies are hidapi (linux-native), eframe, serde, serde_json, anyhow, dirs,
and image (png only, decodes the bundled device render for the sidebar), and
egui-phosphor 0.11 (effect icons; 0.11 is the release built on egui 0.33). Cargo.lock
is committed. The GUI bundles Space Grotesk, Work Sans and JetBrains Mono (OFL) in
`assets/fonts/`; Export/Import use `zenity` for the file dialog.

Read-only sources used: ref/02 (builders and tables), ref/03 (macro encoding),
ref/06 (operation order and transport), and the missing effect catalogue and
DeviceKeyCode constructor from src/out/main/index.js. No Electron code executes.
The brief supplied the verified device identity and hidapi report convention;
this implementation did not independently inspect or access the keyboard.

The 75% ANSI grid is a handwritten row/width table using the supplied physical
mapping. The keybed geometry in `src/ui/widgets.rs` comes from the Figma redesign. The
six screens provide all requested controls. Advanced editing is intentionally
minimal: four numeric-keycode slots, DKS modes/point, mod-tap timeout, toggle type,
and text macro events. It is not a graphical macro timeline editor.

Profiles/layers are zero-based internally, 1–3 in the Device UI and CLI. The
worker selects the saved target profile/layer before un-indexed setting writes.
Sidebar profile/layer selection only browses local saved settings/drafts; it sends
no device command and does not persist a selection. CLI `profile` still explicitly
switches the board with 0x05. Apply selects its edited target before writing.
Explicit Re-apply everything restores local settings, beginning with 0x05 →
0x1D Custom → global/per-key 0x1A → main/side preset → all colour/mode pages.
Preset-only restoration sends zero mode bytes to clear old overrides. Switching
paint mode also sends the desired pages even when no keys are painted, so Apply
can save the mode change. Linked side-light settings synchronize before Apply,
clearing only the side override modes so custom key and badge colours survive.
An independent side-colour edit selects the Single Color/custom RGB preset and
updates remembered side LED colours even while per-key paint is off. Saved side
overrides keep the side lights independent when their preset matches the main one.
CLI `effect` first reads the actual active slot.

Startup performs no automatic writes. The worker opens hardware only on a
command. It filters 342d:e3de by usage_page 0xFFFF, rejects zero/multiple matching
interfaces, and never selects by interface index. It uses feature reports with
65 bytes [0, 64 command bytes], requires a full 65-byte feature response, removes
the leading byte and validates the echoed command. Live reads are zero-timeout
interrupt reads on the same handle, recognize [5,27,value], and clamp at 40.
Each worker poll drains up to 256 queued reports and publishes only the newest
travel sample, rather than replaying one old report every 20 ms. Non-travel
reports do not replace the last travel sample. Enabling live view discards old
queued samples first; the UI ignores late samples after it is switched off.

`SEND_WAIT=550 ms`, `GET_WAIT=150 ms`, `KEYSTROKE_WAIT=650 ms`, and
`COLOR_START_WAIT=750 ms` are in device.rs. The source uses shorter per-page RGB
waits; this implementation conservatively retains 550 ms for them. UI writes
use a worker/channel with report-count progress. Closing the GUI waits for an
in-flight batch and sends visualization-off before joining the worker. A driver
call itself can still block: no unsupported hidapi cancellation is attempted.

Dry-run is selected by `--dry-run` or `GLORIOUS_CORE_DRY_RUN=1` / `true`.
It never creates a HidApi, opens a hidraw handle or writes profile files. It
prints every feature report with the zero prefix and simulates zero read replies.
No GUI, Xvfb, broadway, udev installation or real-device command was run here.
All generated artifacts, build outputs and the writable Cargo cache stay inside
this new project. The original Cargo cache was read/copied because it is read-only
in this environment; `.cargo-home/` and `target/` are ignored by Git.

## Source quirks preserved

- Ordinary checksum is 255 minus the low-byte sum of command bytes 0–6, at 7.
  Payload bytes never participate. GetProfileIndex's original leading-zero
  checksum slice omits command byte 6, which is zero: normalized bytes agree.
- `PrepareLightingBuffers` is **not** the ordinary checksum layout. The seven
  LightingEffect bytes occupy 1–7 (including blue at 7); checksum is at **8**.
  Both 0x07 and 0x08 use this nine-byte header, with zero-filled tail.
- Effect UI values are remapped through MapEffectValue$1; colour modes 0–9 and
  speed/brightness ceiling arithmetic are preserved. RGB is three bytes; no
  extra alpha channel is exposed (equivalent to original alpha=100%).
- The 168-colour hardware array is 504 bytes. Original Paginate's `pad=8` is a
  **minimum**, not a maximum. Actual JS yields **nine** 56-byte colour pages
  (0–8), followed by **three** mode pages. Dropping page 8 would lose the badge
  and some side LEDs. Sides occupy 128+i*3 through 157; badge is 158.
- Travel/rapid/deadzone values use round(mm*10)-1, clamped 1–40. Missing deadzone
  sends 40. The original per-key pass omits deadzone; this port does too.
- DKS sends only the first nonzero press point, falling back to the second or
  4 mm, and clamps it 4–24. It sends four separately shifted mode bytes rather
  than OR-ing them together. A second independent threshold is not fabricated.
- Macro long delay's high byte uses **>>4**, not >>8. MacroSequence keeps its
  repeat/zero prefix, zero/zero terminator, 56-byte pages and five-page cap.
  PrepareMacroData has stable time ordering, minimum one-millisecond delays and
  ignores initial silence, matching JS. Values beyond safe timestamp/delay
  ranges are rejected instead of overflowing. Unknown named GUI keys are errors.
- KeyCodeMapping is copied faithfully, including MetaLeft/MetaRight = 0. The
  original modifier-combination system uses different values; the minimal GUI
  offers the supplied table and does not invent Windows/meta remapping behaviour.

## Builder/test inventory

Tests use hand-calculated literal byte prefixes plus an asserted zero tail.
They are independent expected data, not JavaScript execution or the Rust
checksum implementation used to generate expected bytes.

| Rust API | JS counterpart | Test in tests/protocol.rs |
|---|---|---|
| checksum | CalculateChecksum | checksum_wrap |
| get_fw | PrepareGetFWVersion (USB/RF/LED) | firmware |
| get_profile / set_profile | PrepareGetProfileIndex / PrepareSetProfileIndex | profiles |
| get_sleep / set_sleep | SleepTimeSettings.toGetReportBuffer / toReportBuffer (USB) | sleep |
| report_rate | PrepareSetReportRate | performance |
| debounce | PrepareDebounceBuffer | performance |
| reset | PrepareReset | dangerous |
| calibration | PrepareSwitchesCalibration, all four stages | dangerous |
| visualization | PrepareVisualization | live |
| custom_actuation | PrepareSetCustomActuationMode | magnetism |
| actuation | PrepareActuationPointAndRapidTriggerMagnetismReport | magnetism |
| keystroke | PrepareSetKeystroke | keystrokes |
| keystroke_clear | PrepareKeystrokeClear + UpdateKeystrokeClear, composed | keystrokes |
| complex_keystroke | handler #setComplexKeystroke, four ordered slots | keystrokes |
| toggle | PrepareToggleMagnetismReport | advanced |
| mod_tap | PrepareModTapMagnetismReport | advanced |
| dynamic_keystroke | PrepareDynamicKeyStrokeMagnetismReport / CreateDKSModes | advanced |
| lighting / map_effect | LightingEffect / PrepareLightingBuffers / MapEffectValue$1 | lights |
| per_key_color / per_key_mode | PrepareSetPerKeyColor / PrepareSetPerKeyColorMode | color_pages |
| per_key_lighting | MagnetonPerKeyLighting / Paginate | color_pages |
| macro_page | PrepareMacroBuffer$2 | macros |
| macro_reports / macro_keycode | MacroSequence / GenerateMacroKeyCode | macros |
| macro_data | PrepareMacroData | macro_timeline |
| get_battery | PrepareGetBatteryStateBuffer (USB only) | wired_battery |
| actuation_reports / activate_reports | handler global/per-key/activation sequence | sequence_and_state |

Parsers test short/malformed responses; state tests cover 3×3 serialization,
validation, backup preservation, atomic replacement and refusing corrupt files.
CLI integration tests cover info, profile, effect, both dry-run selectors,
65-byte framing, invalid inputs, no state-file creation, and worker progress.

Deliberately excluded PrepareBTBuffer and receiver-specific request/response
branches, dongle reads, firmware updating, cloud/accounts, RGB sync and audio,
as the brief requires. The RF firmware-version opcode 0x80 remains a wired
feature query to the keyboard, not a receiver transport. The USB battery builder
is covered as a source helper but is not part of the minimal Device info view.

## Command effects: reads versus writes

There is **no explicit flash-commit opcode** in the supplied handler. Settings
apply live. The JS alone does not establish exactly when firmware programs
physical flash, nor which settings survive unplugging. Consequently the honest
classification below is exact for **configuration writes versus read queries**;
physical flash persistence is unverified. Treat all configuration operations as
potential flash writes until firmware/hardware testing establishes otherwise.

| Command | Operation | Classification |
|---|---|---|
| 0xF0 / 0x80 / 0xAE | Firmware version queries | Read-only; query packets sent, no setting change |
| 0x85 | Active profile/layer query | Read-only |
| 0x92 | Sleep timeout query | Read-only |
| 0x83 | USB battery query (helper only) | Read-only |
| 0x05 | Select profile/layer | Changes active board state; may persist |
| 0x07 / 0x08 | Main/side preset lighting | Configuration write, potentially flash |
| 0x0C / 0x51 | Per-key colour/mode pages | Configuration write, potentially flash |
| 0x1D | Custom actuation mode | Configuration write, potentially flash |
| 0x1A | Global/per-key actuation, HE clear, DKS, mod-tap, toggle | Configuration write, potentially flash |
| 0x04 | Polling rate | Configuration write, potentially flash |
| 0x11 | Debounce/input latency | Configuration write, potentially flash |
| 0x12 | USB sleep/deep sleep timer | Configuration write, potentially flash |
| 0x13 | Keystroke slots | Configuration write, potentially flash |
| 0x16 | Macro pages | Configuration write, potentially flash |
| 0x1B | Visualization on/off | Runtime reporting control; not a read-only query |
| 0x1C / 0x1E | Calibration stages | Destructive calibration write; typed gate |
| 0x02 | Factory reset | Destructive configuration write; typed gate |

CLI `info` sends only read-only queries. CLI `profile` writes only the profile/layer
selection. CLI `effect` reads the active slot then writes 0x07. GUI Apply
batches also select profile/layer first (0x05) when needed. The live checkbox and
normal shutdown may write 0x1B. Reset/calibration only run after typing the exact
confirmation; calibration never advances itself. Neither is exposed by the CLI.

## Remaining hardware/GUI limits and risks

- All hardware timings, feature replies, actual LED mapping/colours, switch
  travel, key output, live reports and persistence across power cycles remain
  untested. GUI was compiled but never launched, per the brief.
- The board is largely write-only. Initial local defaults do not describe the
  current hardware. Re-apply everything explicitly restores local settings.
  Per-key painting writes the entire 168-entry array, including zeros for
  unpainted positions. A partial batch cannot be rolled back automatically.
- External profile changes (keyboard hotkeys/another application) are not
  subscribed to; explicit Device reads resynchronize the editor selection. Apply selects
  its local target. Concurrent app instances are unsupported; use one writer.
- State JSON is backed up and replaced only after successful report sends.
  Send success is not firmware acceptance or power-cycle persistence proof.
  Backups accumulate. Corrupt state files are refused and preserved, not reset.
  Dry-run intentionally does not persist previews. Errors drop cached device handles;
  the next explicit action reopens them. Failed batches are never retried automatically.
- Reset deliberately retains saved local settings. Restore them deliberately
  with Apply or Re-apply everything; browsing profiles/layers never changes the board.
- Updating global actuation writes ordinary magnetism data and can replace a
  selected key's advanced mode. Apply the advanced binding again afterward.
- GUI macro editing is raw event text; it does not record input, balance press/
  release automatically, or reserve IDs. Review events before applying.
- Source-derived colour page count and shifted macro delay are preserved even
  though surprising. Only testing on the target can validate firmware quirks.
- Calibration has no illustrated physical procedure; all four stages require
  deliberate separate typed confirmation. Do not guess a calibration sequence.

## Verification completed (2026-10-18)

- `cargo build --release`: passed.
- `cargo test`: passed, **22 tests** (17 protocol/table/state tests,
  4 dry-run CLI/worker tests, 1 filesystem persistence test).
- `cargo clippy --all-targets -- -D warnings`: passed, no warnings or errors.
- Release executable `--dry-run info`: passed; logged the five expected 65-byte
  requests and explicitly synthetic version/profile/sleep replies.
- Commands above used this project's ignored `.cargo-home` via CARGO_HOME to
  keep dependency downloads/build-cache changes within the allowed directory.
- No GUI launched. No keyboard enumerated, opened, read or written. No udev rule
  installed. Hardware usability and flash persistence are not claimed verified.

## GUI: Omarchy theming and Hyprland layout

- Colours come from the active Omarchy theme, `~/.local/state/omarchy/current/theme/colors.toml`
  (`src/ui/theme.rs`). Every token is a mix of the theme background and foreground, the
  ladder the Omarchy shell uses for its controls, plus the theme accent, green and red. Small
  dim text is held at WCAG AA (4.5:1) against cards. Light themes (`mode = "light"`) switch
  egui to light visuals. Without a theme the Figma palette is reproduced exactly.
- A thread stats `colors.toml` every 2 s and wakes the UI only when it changed, so theme
  switches apply live and an idle window draws no frames. The device worker channel is polled
  only while a command is running or live view is on.
- `keyboard.rgb` from the theme is offered as the first lighting swatch.
- Layout: the 248 px sidebar (device image, labelled nav) always shows; in narrow tiles card
  rows wrap (`widgets::flow`), toolbars wrap, and the keybed scales down to 55 %, scrolling
  sideways inside its card only below that. Labels elide instead of widening their cards.
- Keyboard: Tab focus rings on every control, arrow keys step focused sliders, Ctrl+1–6
  screens, Ctrl+S / Ctrl+Enter apply, Ctrl+Z revert, Ctrl+R or F5 re-read the device.
- zenity file dialogs run on their own thread. Omarchy's default window opacity
  (0.985 / 0.96) applies to this window like any other; opting out needs a Hyprland rule.
