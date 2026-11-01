# Plan

What to build next, simplest first. `ARCHITECTURE.md` has the target design.

- One step at a time. Before starting a step, expand it here into small sub-steps, each with a check and a commit.
- When a decision changes, update `ARCHITECTURE.md` in the same commit.
- "Decide first" items need the user's answer before any code.
- A check that needs a board can wait: the step gets committed anyway, and the check moves to "Open hardware checks" at the end.

## 3. Decoder and `cargo xtask schemas`

Turn packets into HA sensors without per-kind receiver code. Branch `step-3-decoder`.

Decided:

- Fixed fields only. The ds18b20's repeated group comes to the schema and the decoder in step 4, together with kind 1 and its golden bytes.
- A field's scale is an integer divisor: value = raw / `divisor`. No floats in `crates/proto`, and JS prints exact values (185 / 50 = 3.7, but 185 × 0.02 = 3.7000000000000002). The battery goes to HA in V (divisor 50).
- `schemas.json` is committed. CI regenerates it and fails if it differs.
- `--publish` uses `rumqttc` without TLS (the broker is on the LAN). The broker comes from `AIO_MQTT_URL`, e.g. `mqtt://user:pass@192.168.1.10:1883`.
- The decoder is one Node-RED function node: you paste `decoder/decode.js` once and wire it by hand. Schema changes arrive over `aio/schemas`, so you re-paste only when `decode.js` itself changes.
- Decoder tests use Node's built-in test runner (`node --test`), no npm packages. Node is on the PC and on the CI runners.
- The decoder also routes legacy packets, by `reserved_node_ids` from `schemas.json`. Removing an ID from `nodes.toml` and publishing reroutes it, with no Node-RED edit.
- State goes to `aio/<node_id>/state`, not retained, so an old value can't come back as new. Discovery is retained.
- Each node's default device always exists and carries RSSI and SNR from OpenMQTTGateway, as diagnostic sensors.
- Moving a source to another device, or removing a node: delete the old device in HA by hand, then `cargo xtask schemas --publish`. The decoder re-creates what's still configured.

### 3.1 Schema fields (`crates/proto`)

- `Type`: `U8`, `I8`, `U16`, `I16`, `U32`, `I32`, with `size()` in bytes. The missing value follows from the type (`MIN` signed, `MAX` unsigned).
- `Field` gets `ty: Type`, `divisor: u32`, `unit`, `device_class` and `state_class` (each `Option<&'static str>`, in HA's names, e.g. `voltage`, `measurement`).
- `Battery::FIELDS`: `voltage`, `U8`, divisor 50, `V`, `voltage`, `measurement`, no threshold.
- Test: the battery body is as long as the sizes of its `FIELDS` add up to, so schema and encoder can't drift apart.

**Check:** `cargo test -p proto`, and the RISC-V `cargo build` line from `ci.yml`. **Commit:** "Add the schema to kind 4"

### 3.2 `cargo xtask schemas` writes `schemas.json`

- `xtask` gets `serde_json`. `Source` gets `ALL` and `kind()`.
- New `xtask/src/schemas.rs`: validate, then write `schemas.json` (pretty, sorted, trailing newline):
  - `kinds`: kind number → source name and fields (key, type, divisor, unit, device_class, state_class). Only sources that have fields. Thresholds stay out; the decoder doesn't need them.
  - `nodes`: node ID → name, heartbeat, sources.
  - `reserved_node_ids`.
- Test: the JSON for a small `nodes.toml`.
- CI host job: `cargo xtask schemas`, then `git diff --exit-code schemas.json`.
- `HELP`; `ARCHITECTURE.md`: a short `schemas.json` example under "Decoding & Home Assistant".

**Check:** `cargo test -p xtask`. `cargo xtask schemas` twice in a row leaves no diff. **Commit:** "Export schemas.json"

### 3.3 HA devices in `nodes.toml`

- `[devices.<key>]` with `name` and `sources = ["<node>.<source>", ...]`; none by default.
- New rules in `validate.rs`, each with a test:
  - Every entry names an existing node that runs that source. The message lists the node's sources.
  - No source in two devices, or twice in one.
  - A device has at least one source.
  - The key matches `[a-z][a-z0-9_-]*`. It becomes `aio_<key>` in HA topics; starting with a letter, it can't clash with a node's `aio_<node_id>`.
- `schemas.json` gets `devices`: key → name and sources as `{ node: <node_id>, source }`.
- `check` prints the devices. `nodes.toml` header comment and `ARCHITECTURE.md` "Node configuration": the syntax, no longer "Planned".

**Check:** `cargo test -p xtask`, `cargo xtask check`. A temporary bad entry (`dev-c6.foo`, or one source in two devices) fails with a clear message. **Commit:** "Add HA devices to nodes.toml"

### 3.4 Publish `schemas.json`

- `xtask` gets `rumqttc` without default features (no TLS).
- `cargo xtask schemas --publish`: write the file, then publish it retained with QoS 1 to `aio/schemas`, and wait for the broker's ack (time out after a few seconds). Here a missing `AIO_MQTT_URL` or an unreachable broker is an error.
- `cargo xtask run`: the same before building, but there it only warns and carries on.
- Test: reading `AIO_MQTT_URL` (host, default port 1883, user and password, other schemes rejected).
- `HELP`; `ARCHITECTURE.md` "Build & commands".

**Check:** `cargo test -p xtask`. With `AIO_MQTT_URL` set, `cargo xtask schemas --publish`, then an MQTT client (MQTT Explorer, or a Node-RED debug node) shows the retained JSON on `aio/schemas`. Without it, `cargo xtask run dev-c6` warns and still flashes. **Commit:** "Publish schemas.json"

### 3.5 Decoder: packets → state (`decoder/decode.js`)

- OpenMQTTGateway's message, as a Node-RED debug node showed it from dev-c6: `{"topic":"omg/openmqttgateway/LORAtoMQTT","payload":{"rssi":-94,"snr":11.25,"pferror":9739,"packetSize":3,"hex":"640440"}}`. The packet is the lowercase hex string in `hex` (always, since the second byte is a kind, which isn't printable). The decoder ignores `pferror` and `packetSize`.
- The body of one function node, plain JS. Two outputs: 1 = legacy packets, unchanged; 2 = MQTT messages (`topic`, `payload`, `retain`).
- `msg.topic` is `aio/schemas`: keep the schemas in flow context.
- Anything else is a packet:
  - No schemas yet: warn, drop.
  - First byte in `reserved_node_ids`: output 1.
  - Walk the sections. Unknown kind, a kind twice, or a wrong total length: warn, drop.
  - Kinds that differ from the node's sources: warn, still publish.
  - Publish `{"battery": {"voltage": 3.7}, "rssi": -71, "snr": 9.5}` to `aio/<node_id>/state`, not retained. Value = raw / divisor; the missing value becomes `null`.
- `decoder/decode.test.js` runs the file the way Node-RED does (`new Function("msg", "node", "flow", code)`) with fake `node` and `flow`. Tests: the message above gives 1.28 V, RSSI −94 and SNR 11.25 with the real `schemas.json`; `64 04 b9` gives 3.7 V; `64 04 ff` gives `null`; every type, negative values and missing values with a test schema; unknown kind; too short and too long; a kind twice; reserved ID → output 1; no schemas yet; a source missing from the packet.
- CI host job: `node --test decoder/`.

**Check:** `node --test decoder/`. **Commit:** "Decode packets in Node-RED"

### 3.6 Decoder: HA discovery

- Publish one retained discovery message per device on every `aio/schemas` message (the retained one also arrives when Node-RED starts) and on `homeassistant/status` = `online`:
  - `homeassistant/device/aio_<node_id>/config` per node, named after the node: its sources that no `[devices.*]` lists, plus `rssi` (dBm) and `snr` (dB) as diagnostic.
  - `homeassistant/device/aio_<key>/config` per declared device.
- Each field is a `sensor` component with its node's state topic, a `value_template`, unit, device_class, state_class and `expire_after` = 2.5 × heartbeat. `unique_id` = `aio_<node_id>_<source>_<field>`; `aio_<node_id>_rssi` and `aio_<node_id>_snr`.
- A node ID that isn't in `schemas.json` but sends known kinds: on its first packet, a default device named `aio <node_id>`, without `expire_after`.
- Tests: dev-c6's discovery from the real `schemas.json`; a declared device takes the source away from the default device, while RSSI and SNR stay; `online` republishes; an unknown node.
- `ARCHITECTURE.md` "Decoding & Home Assistant": RSSI and SNR, their `unique_id`s, and how to move a source or remove a node by hand.

**Check:** `node --test decoder/`. **Commit:** "Publish HA discovery"

### 3.7 Set up Node-RED

- The header comment of `decode.js` says how to wire it, once:
  - MQTT in on `aio/schemas`, `homeassistant/status` and `omg/openmqttgateway/LORAtoMQTT`, all with "auto-detect", into the function node (2 outputs, `decode.js` pasted in).
  - Output 1 → the legacy flow, in place of its own MQTT in. Output 2 → MQTT out (topic and retain from the message).
- Do it in the real Node-RED, then `cargo xtask schemas --publish`.
- Try the manual move once: add `[devices.test]` with `dev-c6.battery`, delete the dev-c6 device in HA, publish. Then undo it the same way.

**Check:** the legacy nodes still show up in HA as before. dev-c6 shows up with battery voltage, RSSI and SNR, and its sensors turn unavailable about 150 s (2.5 × 60 s) after unplugging it. The test device gets the battery, and dev-c6 keeps RSSI and SNR. **Commit:** "Set up the decoder in Node-RED" (only fixes that come out of it)

### 3.8 Wrap up

- `ARCHITECTURE.md` "Built so far". Move the board checks that are still open to "Open hardware checks", and delete step 3 from this plan. Merge `step-3-decoder` into `main` once CI passes.

**Check:** the step's goal: dev-c6 shows up in HA with its battery sensor, and turns unavailable when unplugged.

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
