# Plan

What to build next, simplest first. `ARCHITECTURE.md` has the target design.

- One step at a time. Before starting a step, expand it here into small sub-steps, each with a check and a commit.
- When a decision changes, update `ARCHITECTURE.md` in the same commit.
- "Decide first" items need the user's answer before any code.

## 1. `battery` source (always-on)

The first real path from sensor to packet. Branch `step-1-battery`.

Decided: kind 4 is one `u8` = battery mV / 20, rounded (0–5.08 V); 0xFF = not measured. The ESP32 ADC isn't more accurate than about 20 mV, and with no offset the decoder only has to scale.

### 1.1 Packet builder (`crates/proto`)

- Add `heapless` to the workspace dependencies and to `crates/proto`.
- `Section` trait: `const KIND: u8`, plus `encode`, which writes the body.
- `Packet` wraps `heapless::Vec<u8, 255>`. `Packet::new(node_id)` writes the first byte. `push(&section)` appends `[kind] [body]`, or returns an error if that would go past 255 bytes. `as_bytes()` returns the result.
- Tests with a test-only kind: one section, two sections, and the 255-byte limit.

**Check:** `cargo test -p proto`, and the RISC-V `cargo build` line from `ci.yml`. **Commit:** "Add the packet builder"

### 1.2 Kind 4 (`crates/proto`)

- `Battery` holds the wire byte. `Battery::from_millivolts(mv: u16)` rounds to 20 mV and clamps to 0xFE (5.08 V). `Battery::NOT_MEASURED` is 0xFF. It implements `Section` with kind 4.
- Golden-byte tests: 0 mV → `00`, 3700 → `B9`, 3710 → `BA` (rounds up), 5080 → `FE`, 6000 → `FE` (clamped), not measured → `FF`. Packet test: node 100 at 3700 mV → `64 04 B9`.

**Check:** `cargo test -p proto`. **Commit:** "Add kind 4 (battery)"

### 1.3 Battery pins (`boards.toml`, xtask)

- New optional group per board: `battery = { adc = <GPIO>, divider = <battery voltage / pin voltage> }`.
  - t3s3: `adc = 1, divider = 2.0` (from `docs/legacy-porting.md`).
  - c6-dev: placeholder values. Set the real pin and divider before flashing.
- `xtask/src/config.rs`: `Board.battery: Option<BatteryPins>`. `Board::pins()` lists `battery.adc`, so the existing pin rules check it, and it gets exported as `AIO_PIN_BATTERY_ADC` like the LoRa pins.
- `xtask/src/firmware.rs`: also export `AIO_BATTERY_DIVIDER_PERMILLE` (divider × 1000, rounded), so the firmware only does integer math. Extend the `pins_become_env_vars` test.
- `firmware/node/src/config.rs`: `BATTERY_ADC` and `BATTERY_DIVIDER_PERMILLE`, behind `source-battery`. Add both vars to `.vscode/settings.json`.

**Check:** `cargo test -p xtask`, `cargo xtask check`. **Commit:** "Add battery pins to boards.toml"

### 1.4 Source rules (xtask)

New rules in `validate.rs`, each with a test:

- `battery.adc` is an ADC1 pin: GPIO 0–6 on the C6, GPIO 1–10 on the S3.
- `divider` is at least 1.
- Each source of a node finds its pins on the node's board (`battery` needs `battery`). One `match` on `Source`, so later sources add one arm.
- A node has at least one source, because a packet holds at least one section. So dev-s3 gets `sources.battery = {}` too.

**Check:** `cargo test -p xtask`, `cargo xtask build --all`. `cargo xtask check` passes, and fails with a clear message when `battery` is removed from c6-dev. **Commit:** "Check source pins in the xtask"

### 1.5 Battery ADC driver (`board/`)

- esp-hal's ADC takes a typed pin (`peripherals.GPIO2`), not an `AnyPin`. So `board/` matches `BATTERY_ADC` against the chip's ADC1 GPIOs with a small macro, and each arm builds the reader for its pin. ADC1 with `AdcCalCurve` calibration and 11 dB attenuation, which returns millivolts.
- Embassy tasks can't be generic, so the reader hides the pin type behind a small trait with `fn read_mv(&mut self) -> u16`, stored in a `StaticCell`.
- `Board` gets the reader behind `source-battery`. `main.rs` passes it to the battery task, which logs the pin voltage every `heartbeat` for now.

**Check:** `cargo xtask build --all`, `cargo xtask clippy dev-c6`. On dev-c6, the logged pin voltage matches a multimeter at the pin. **Commit:** "Read the battery ADC"

### 1.6 Battery task

- Every `heartbeat`: average 8 readings, multiply by the divider, convert with `Battery::from_millivolts`, and log the battery mV. The interval stays `heartbeat` until step 2 brings the send policy.
- Store the result in `pub static LATEST: Mutex<CriticalSectionRawMutex, Cell<Battery>>` (starts as `NOT_MEASURED`), then signal `report::NEW_DATA` (an embassy-sync `Signal`).

**Check:** on dev-c6, the logged battery mV is within about 50 mV of a multimeter at the battery. If it's further off, fix `divider` in `boards.toml`. **Commit:** "Sample the battery"

### 1.7 Real packets (report task)

- The report task waits for `NEW_DATA`, then builds `Packet::new(NODE_ID)` with one section per source the node runs (`#[cfg(feature = "source-…")]`), and sends it. The test bytes go.
- `ARCHITECTURE.md`: update "Built so far".

**Check:** the gateway shows `64 04 xx` from dev-c6 every `heartbeat`, and `xx` × 20 mV matches the multimeter. **Commit:** "Send the battery packet"

### 1.8 Wrap up

- Delete step 1 from this plan. Merge `step-1-battery` into `main` once CI passes.

**Check:** the step's goal: the gateway shows `64 04 xx` from dev-c6, and `xx` matches a multimeter.

## 2. Send policy

- `crates/policy`: thresholds, `min_interval`, heartbeat, as in `ARCHITECTURE.md`. LP-compatible (no floats, no fmt).
- Default thresholds per field in `crates/proto`; overrides in `nodes.toml`.
- The report task uses it.

**Check:** host tests pass. On the board, a change is sent within `min_interval`; without changes, only heartbeats arrive.

## 3. Decoder and `cargo xtask schemas`

- Schema registry in `crates/proto`; `cargo xtask schemas` writes `schemas.json`; `--publish` sends it to `aio/schemas`; `run` publishes before flashing.
- `decoder/`: generic Node-RED function, state JSON, HA discovery.
- `[devices.*]` in `nodes.toml`. xtask: reject a device entry naming a missing `node.source`, or a source listed in two devices.
- Node-RED: route `reserved_node_ids` to the legacy flow.

**Check:** dev-c6 shows up in HA with its battery sensor, and turns unavailable when unplugged.

## 4. `ds18b20` (always-on)

- **Decide first:** `sensor_id`. Bits 48..63 of the ROM code keep the legacy Node-RED mappings, but on genuine Maxim sensors they are `00` plus the CRC, so two sensors on one node can collide (about 6 % with 6 sensors, 23 % with 12). The alternative is an ID derived from the serial number.
- `crates/onewire`: ROM search, CRC8, scratchpad, with host tests. Kind 1 with golden bytes.
- Firmware source on the HP core; 1-Wire pins in `boards.toml`; `max_per_bus` parameter.
- xtask: reject a worst-case packet over 255 bytes (1-Wire buses × `max_per_bus`) and more than one repeated-group source per node. Print worst-case airtime and the duty cycle at `min_interval`; warn above 1 %.

**Check:** the sensors appear in HA. Unplugging one removes it from the next packet.

## 5. `eder` and `pyromat`

- **Decide first:** the field types of kinds 2 and 3.
- `crates/eder` and `crates/pyromat`, with host tests from the facts in `docs/legacy-porting.md`.
- Firmware sources over UART and TWAI. xtask: reject a listening source on a deep-sleep node.

**Check:** the values match the legacy flow side by side. Then delete their sections in `docs/legacy-porting.md`.

## 6. Deep sleep and the coprocessor

- HP core: wake, sample, policy, send, sleep; backup timer of 2 × `heartbeat`.
- `crates/lp-shared` layout; `firmware/lp-tempsense` (S3 ULP via esp-hal, C6 LP core with its own LP-timer wake and halt).
- xtask: build `lp-tempsense` first; reject 1-Wire pins that aren't LP/RTC-capable on deep-sleep nodes.

**Check:** the sleep current is low, and the node sends on change and on heartbeat.

## 7. Reflash the legacy nodes

- temp-stations = `battery` + `ds18b20`, deep sleep; eder gateway = `eder`; pyromat gateway = `pyromat`.
- One node at a time: reflash, then remove its ID from `reserved_node_ids`.
- When all are done: delete the legacy Node-RED flow and `docs/legacy-porting.md`.

**Check:** every node shows up in HA, and `reserved_node_ids` is empty.
