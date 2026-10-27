# CLAUDE.md

## Behaviour

Explain things compactly, in easy language. Plan in small steps that a new Rust/embedded developer can follow.

## Goal

One `no_std` Rust codebase for the LoRa monitoring nodes of a home heating setup. Each node reads local data sources and sends its complete dataset over raw LoRa:

- **temp-station:** DS18B20 sensors, battery-powered, mostly in deep sleep.
- **eder-gateway:** Eder MultiControl pressure-maintenance unit, over UART.
- **pyromat-gateway:** Köb Pyromat ECO wood boiler, over CAN.

OpenMQTTGateway receives the packets, Node-RED decodes them for Home Assistant. Long-term, HA creates a device with sensors for every node automatically; one node can feed several devices, and one device can combine several nodes.

Replaces the ESP-IDF projects `temp_station_feather_s3`, `temp_station_t3s3`, `ederToLora` and `pyromat_to_lora` in `../../ESP-IDF/` (`heatingCircuitMonitor` is out of scope). Existing ESP32-S3 boards stay in service, new nodes use ESP32-C6, no other chips.

Priorities, in order:

1. Little LoRa airtime.
2. Simplicity: every packet carries the node's complete dataset, and messages are never split.
3. Low power on battery nodes.
4. Correct data, even where that means not copying legacy bugs.

## Where to look

- `ARCHITECTURE.md`: decisions, layout, commands, LoRa protocol. Read it before changing code; update it when a decision changes.
- `plan.md`: what to build next.
- `docs/legacy-porting.md`: facts from the legacy projects. Delete each section once its port has landed.
