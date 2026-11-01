# Plan

What to build next, simplest first. `ARCHITECTURE.md` has the target design.

- One step at a time. Before starting a step, expand it here into small sub-steps, each with a check and a commit.
- When a decision changes, update `ARCHITECTURE.md` in the same commit.
- "Decide first" items need the user's answer before any code.
- A check that needs a board can wait: the step gets committed anyway, and the check moves to "Open hardware checks" at the end.

## 4. `ds18b20` (always-on)

- **Decide first:** `sensor_id`. Bits 48..63 of the ROM code keep the legacy Node-RED mappings, but on genuine Maxim sensors they are `00` plus the CRC, so two sensors on one node can collide (about 6 % with 6 sensors, 23 % with 12). The alternative is an ID derived from the serial number.
- `crates/onewire`: ROM search, CRC8, scratchpad, with host tests. Kind 1 with golden bytes.
- The repeated group in the schema, `schemas.json` and the decoder; the decoder republishes discovery when a new `sensor_id` shows up.
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
