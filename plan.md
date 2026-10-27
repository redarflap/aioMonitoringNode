# Plan

What to build next, simplest first. `ARCHITECTURE.md` has the target design.

- One step at a time. Before starting a step, expand it here into small sub-steps, each with a check and a commit.
- When a decision changes, update `ARCHITECTURE.md` in the same commit.
- "Decide first" items need the user's answer before any code.

## 1. `battery` source (always-on)

The first real path from sensor to packet.

- **Decide first:** kind 4's field type, and the ADC pin and divider per board (`docs/legacy-porting.md` has feather-s3 and t3s3).
- `crates/proto`: kind 4, and the packet builder `[node_id] { [kind] [body] }+`, with golden-byte tests and one multi-section test.
- Firmware: the battery task reads the ADC, writes a snapshot, signals the report task. The report task sends the real packet instead of the test bytes.
- `boards.toml`: battery pin. xtask: reject a source whose pins the board doesn't define.

**Check:** the gateway shows `64 04 xx` from dev-c6, and `xx` matches a multimeter.

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
