# Legacy porting reference

These notes cover the ESP-IDF projects in `../../ESP-IDF/`. Most of those projects' READMEs are unedited ESP-IDF boilerplate, so the sources are the reference: `main/*.c(pp)`, `main/config.h`, `main/include/servicemanager.h`, `main/include/ederbuffer.h` and `sdkconfig`. Delete each section here once its port has landed.

The legacy LoRa layouts only matter while not-yet-reflashed nodes are still sending, and the current Node-RED flow already decodes them. They are defined in:

- `App::sendLora()` in `ederToLora/main/app.cpp`: the 8 bytes of `esp_efuse_mac_get_default()` (the 6-byte MAC, then `00 00`), followed by `serializeBytes()` from `ederbuffer.h`. `LORA_NODE_ID` (0x03) is never used.
- the `buffer` assembly in both `temp_station.c` files
- `OvenData_s` in `pyromat_to_lora/main/twai_network_example_master_main.c`

So legacy packets start with: pyromat `01 00`, t3s3 `02 00`, feather-s3 `00` and the last MAC byte, eder `7C DF A1 F9 01 A0 00 00` (its MAC, then the two zero bytes). The current Node-RED flow recognises eder packets by that whole 8-byte prefix.

## temp-station (`temp_station_feather_s3`, `temp_station_t3s3`)

How the legacy version works:

1. **Cold boot:** the HP core enumerates the DS18B20s on each bus (RMT 1-Wire, up to N per bus, 12-bit resolution) and writes the ROM table into ULP memory.
2. It starts the ULP with period `POLL_RATE` and deep-sleeps, with the ULP and a backup timer as wake sources.
3. **Each ULP period:**
   - It sends Skip-ROM + Convert-T on every bus, waits 750 ms, then does Match-ROM + scratchpad read for each known ROM.
   - On the feather-s3, it wakes the HP core if any of these holds:
     - a valid reading (raw −800..3200, i.e. −50..200 °C) changed by at least `MIN_CHANGE` raw units (1/16 °C)
     - `MINIMUM_REPORT_PERIOD` has elapsed
     - this is the first run
   - On the t3s3, it wakes the HP core after every poll. The wake call sits outside the change check ("Send always while testing"), and each run first resets the stored readings to `INT32_MIN`, so `MIN_CHANGE` never takes effect.
4. **On wake,** the HP core sends one packet and goes back to deep sleep.

Legacy problems to fix, not port:

- The ULP reads only the first 2 scratchpad bytes and never checks the CRC. Read all 9 bytes and check CRC8.
- The feather-s3 ignores its `LORA_NODE_ID` (0x02) and sends `mac[6]`, `mac[5]` instead. `mac[6]` is always 0x00, because the MAC has only 6 bytes. Only the t3s3 sends node ID 0x02.

Temperature conversion in the legacy packets: raw × 0.625 gives 0.1 °C. The new format sends the raw value. The feather-s3 battery reading is ADC1 ch1 (GPIO2), 12 dB attenuation, averaged over 5 samples, ×4.014 divider.

Legacy parameters, to become the initial `nodes.toml` entries:

| Param | feather-s3 | t3s3 |
|---|---|---|
| buses × max sensors per bus | 4 × 3 | 3 × 3 |
| poll / min report / backup timer | 120 s / 1800 s / 3600 s | 60 s / every poll / 1800 s |
| `MIN_CHANGE` (raw) | 8 (0.5 °C) | 2 (0.125 °C), never applied |

`MIN_CHANGE` means "at least", while the new thresholds mean "more than" (in the same raw units), so the initial threshold is `MIN_CHANGE` − 1.

## eder-gateway (`ederToLora`)

The gateway reads an Eder MultiControl over UART at 38400 8N1. Each telegram is 192 bytes, with fields **big-endian** at fixed offsets. The full offset/unit/scale map is in `ederToLora/main/include/ederbuffer.h`. The legacy code finds telegrams by counting bytes and throws away 72-byte bursts that arrive while its buffer is empty. Those bursts are almost certainly telegram tails, not a second message type: the UART driver runs with ESP-IDF defaults, so it hands over each telegram as 120 bytes (RX FIFO full) and then 72 (idle timeout), and dropping a lone tail is how the legacy code resyncs. Prefer to delimit telegrams with the UART RX idle timeout.

Legacy send policy:

- Send immediately when the error, refill or pump1 flag changes.
- Otherwise send when there is new data and at least 30 s have passed since the last send.
- After 60 s without a send, deep-sleep for 30 s, which amounts to a reboot.

Legacy bug: `getFlag(buf, byte, bit)` treats `bit` as a mask, not a bit index, so the decoded flags are wrong:

| Flag | Legacy decode |
|---|---|
| warning | always false |
| error | bit0 |
| refill | bit1 |
| pump1 | bits 0\|1 |
| rapid degassing | bits 0\|2 |

Decode with `1 << bit`, and check the result against a real captured telegram. The Home Assistant history still holds the wrong values.

## pyromat-gateway (`pyromat_to_lora`)

The gateway reads the CAN bus of a Köb Pyromat ECO wood boiler: TWAI at 125 kbit/s, standard IDs, accepting all frames. It only parses frames with an ID strictly between 0x200 and 0x600, and it ignores the ID itself. In those frames:

- `data[0..2]` is a little-endian `u16` register index.
- `v1`, `v2` and `v3` are little-endian `u16` values at `data[2..4]`, `[4..6]` and `[6..8]`.

All temperatures are in 0.1 °C. The units of the other fields are still unknown.

| Index | Fields |
|---|---|
| 0x1e | v3 restO2 (`i16`) |
| 0x42 | v2 clockM |
| 0x44 | v2 clockH, v1 clockS |
| 0x66 | v1 tempExhaust, v3 ventilatorRpm |
| 0x7b | v3 valveReturnMixing (unverified) |
| 0x7e | v2 valveBuffer |
| 0x8a | v1 airInletPrimary (90 = open; range unverified) |
| 0x8c | v3 targetRestO2 |
| 0x8d | v3 tempReturn (unverified) |
| 0x92 | v2 airInletSecondary (unverified) |
| 0x9c | v3 tempSystem |
| 0x9f | v1 targetSystemTemp |
| 0x171 | v2 tempBufferTop, v3 tempBufferMiddle |
| 0x174 | v1 tempBufferBottom |

How the legacy node behaves:

- It sends every 30 s, the first packet after 20 s.
- After each send it resets every field to 0. In the port, keep the last value and use the sentinel once a field is stale.
- It runs TWAI in normal mode, so it ACKs frames even though it never transmits. Use listen-only mode so the node can never disturb the boiler's bus.

## Board pin maps

These become the initial `boards.toml` entries.

| Board | Chip / radio | LoRa | Other |
|---|---|---|---|
| feather-s3 (temp station and eder gateway) | S3 / SX127x | MISO 37, MOSI 35, SCK 36, CS 5, RST 6; DIO0 wired, GPIO **unknown** | 1-Wire 1, 3 (strapping pin), 7, 10; battery ADC1 ch1 (GPIO2); eder UART TX 18 / RX 17 |
| t3s3 (LilyGo T3-S3) | S3 / SX1262 | MISO 3, MOSI 6, SCK 5, NSS 7, RST 8, BUSY 34, DIO1 33 | 1-Wire 12, 15, 16; SSD1306 I²C SCL 17 / SDA 18 at 100 kHz; battery divider (~2:1) on GPIO1 |
| pyromat (S3) | S3 / SX127x | **unknown** | **unknown** |

The legacy drivers polled the radio and the t3s3 never read its battery, so the t3s3's DIO1 and battery pins come from LilyGo's pin map (as used by Meshtastic), not from legacy code.

The deployed pyromat board is an S3, but the `sdkconfig` in `pyromat_to_lora` targets a classic ESP32: LoRa MISO 19, MOSI 27, SCK 5, CS 18, RST 23; TWAI TX 13 / RX 12. GPIO27 is a flash pin on the S3, so those pins can't be the S3 ones. The Kconfig's S3 defaults are TWAI TX 42 / RX 41. The real pin map isn't known yet. Fill in the `boards.toml` entry once the user has it; no code change is needed.
