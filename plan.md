# Plan

What to build next, simplest first. `ARCHITECTURE.md` has the target design.

- One step at a time. Before starting a step, expand it here into small sub-steps, each with a check and a commit.
- When a decision changes, update `ARCHITECTURE.md` in the same commit.
- "Decide first" items need the user's answer before any code.
- A check that needs a board can wait: the step gets committed anyway, and the check moves to "Open hardware checks" at the end.

## 2. Send policy

Send only what's worth the airtime: changes, and a heartbeat. Branch `step-2-send-policy`.

Decided:

- Threshold overrides go into `nodes.toml` as `sources.<name> = { thresholds = { <field> = <wire units> } }`, apart from the source's other parameters.
- A field's default threshold can be "none": then it never triggers a packet and only rides along. `nodes.toml` can still set a number.
- `battery` keeps sampling every `heartbeat`, and its field `voltage` has no threshold. On deep-sleep nodes the battery can't trigger anything anyway: the HP core reads it only once it's awake to send.
- `heartbeat` counts from the last packet, whatever caused it.
- After boot, the first packet goes out right after the first sample.
- A failed send keeps the old "last sent" values, so the change goes out again after `min_interval`.
- dev-c6 only has a battery, so it can't show a change-triggered packet. Step 4 checks that on the board, with the DS18B20s.

### 2.1 Policy rules (`crates/policy`)

- `changed(last: Option<i64>, now: Option<i64>, threshold: Option<u32>) -> bool`. A `None` value is missing.
  - No threshold: never a change.
  - Both values missing: no change. One missing: change.
  - Both there: change if they differ by more than the threshold. `i64`, so every wire type up to `u32` fits.
- `Policy { min_interval_s, heartbeat_s }` with `decide(since_last_send_s: Option<u32>, changed: bool) -> Decision`. `Decision` is `Send`, or `Wait(s)`: ask again after `s` seconds, or sooner when new data arrives.
  - nothing sent yet → `Send`
  - `heartbeat` passed → `Send`
  - changed and `min_interval` passed → `Send`
  - changed, `min_interval` not passed yet → `Wait(rest of min_interval)`
  - otherwise → `Wait(rest of heartbeat)`
- No dependencies, no floats, no `core::fmt`. A test per rule, plus threshold 0, negative values, and the edges (exactly at the threshold, exactly at `min_interval`).

**Check:** `cargo test -p policy`, and the RISC-V `cargo build` line from `ci.yml`. **Commit:** "Add the send policy"

### 2.2 Fields and default thresholds (`crates/proto`)

- `proto` depends on `policy`.
- `Field { key: &'static str, threshold: Option<u32> }`: a field's name and its default threshold in wire units. Step 3 adds type, scale and unit, and this becomes the schema registry.
- `Section` gets `const FIELDS: &'static [Field]` (wire order) and `fn changed(&self, last: &Self, thresholds: &[Option<u32>]) -> bool`, which calls `policy::changed` per field. The test-only `Fill` has no fields and never changes.
- `Battery`: `FIELDS = [voltage, no threshold]`; `NOT_MEASURED` becomes a missing value.
- Tests: with no threshold, nothing is a change, not even measured ↔ not measured. With threshold 2: 3700 → 3740 mV (2 steps) isn't a change, 3700 → 3760 mV (3 steps) is, measured ↔ not measured is.
- `ARCHITECTURE.md` "Send policy": a field may have no threshold, like the battery.

**Check:** `cargo test -p proto`, and the RISC-V `cargo build` line. **Commit:** "Add fields and thresholds to kind 4"

### 2.3 Threshold overrides and interval rules (`nodes.toml`, xtask)

- `xtask` depends on `proto`. `Source::fields()` returns its kind's `FIELDS`; sources not ported yet have none.
- `SourceParams` gets `thresholds: BTreeMap<String, u32>` (empty by default), so `sources.battery = { thresholds = { voltage = 2 } }` parses.
- New rules in `validate.rs`, each with a test:
  - Every threshold key is a field of its source. The message lists the source's fields.
  - `1 ≤ min_interval ≤ heartbeat`. (A `heartbeat` of 0 would send in a loop.)
- `firmware.rs`: for every source with fields, export `AIO_THRESHOLDS_<SOURCE>`: its thresholds in `FIELDS` order, comma-separated, defaults with overrides applied, `-` for none (`AIO_THRESHOLDS_BATTERY=-`). Test it, with and without an override.
- `firmware/node/src/config.rs`: `BATTERY_THRESHOLDS: [Option<u32>; Battery::FIELDS.len()]`, read by a new `const fn parse_list` (digit by digit, `-` is `None`). A wrong count fails the build. Add the var to `.vscode/settings.json`.
- `nodes.toml` header comment and `ARCHITECTURE.md` "Node configuration": the `thresholds` syntax.

**Check:** `cargo test -p xtask`, `cargo xtask check`, `cargo xtask build --all`. `cargo xtask check` fails with a clear message for `thresholds = { foo = 1 }` and for `min_interval = 0`. **Commit:** "Pass thresholds to the firmware"

### 2.4 Report task uses the policy (firmware)

- `firmware/node` depends on `policy`.
- `report.rs`: a `Snapshot` holding each source's latest section (one `cfg` field per source). `Snapshot::latest()` reads the `LATEST` cells, `changed(&last)` asks each section with its `config::*_THRESHOLDS`, `packet()` builds the packet.
- The loop: wait for the first `NEW_DATA`. Then take a snapshot and ask `decide`, with the time since the last send from `embassy_time::Instant`.
  - `Send`: send, then remember the snapshot and the time. If the send fails, remember only the time.
  - `Wait(s)`: `embassy_time::with_timeout(s, NEW_DATA.wait())`, then ask again.
- Log each send with its reason (change or heartbeat).
- The battery task's timer and the heartbeat run apart, so a packet may carry a battery sample up to one `heartbeat` old. That's fine for a battery.
- `ARCHITECTURE.md`: "Built so far", and the policy details from "Decided" above.

**Check:** `cargo xtask build --all`, `cargo xtask clippy dev-c6`. On dev-c6 (heartbeat 60 s): a packet arrives every 60 s, never more often, even when the battery voltage changes. **Commit:** "Send by the policy"

### 2.5 Wrap up

- Move the board checks that are still open to "Open hardware checks", and delete step 2 from this plan. Merge `step-2-send-policy` into `main` once CI passes.

**Check:** the step's goal: host tests pass. On dev-c6, packets arrive only every `heartbeat`.

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

**Check:** the sensors appear in HA. Unplugging one removes it from the next packet. A temperature change is sent within `min_interval`.

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

## Open hardware checks

Built and committed, but not yet tried on a board. Delete each line once it passes.

### `battery` on dev-c6 (step 1)

First set the real battery pin and divider of `c6-dev` in `boards.toml`, then `cargo xtask run dev-c6`.

- The logged pin mV matches a multimeter at the ADC pin.
- The logged battery mV is within about 50 mV of a multimeter at the battery. If it's further off, fix `divider` in `boards.toml`.
- The gateway shows `64 04 xx` from dev-c6 every `heartbeat`, and `xx` × 20 mV matches the multimeter.
