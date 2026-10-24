# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Behaviour

Try to explain everything in compact and easy language. Make plans in small incremental steps that are easy to follow for a new rust/embedded developer.

## Goal

One `no_std` Rust codebase for the LoRa monitoring nodes of a home heating / home-automation setup. Each node reads one or more local data sources and sends a compact, complete dataset over raw LoRa. Today's node types:

- **temp-station:** DS18B20 temperature sensors on a battery-powered node that spends most of its time in deep sleep.
- **eder-gateway:** an Eder MultiControl pressure-maintenance/degassing unit, read over UART.
- **pyromat-gateway:** a Köb Pyromat ECO wood boiler, read over its CAN bus.

An OpenMQTTGateway receives the packets, and Node-RED decodes them and passes them to Home Assistant. Long-term goal: Home Assistant automatically creates devices, and their sensors, for every node that sends data. One node can feed several devices, and one device can combine sources from several nodes.

This repo replaces these ESP-IDF C/C++ projects in `../../ESP-IDF/`: `temp_station_feather_s3`, `temp_station_t3s3`, `ederToLora` and `pyromat_to_lora`. `heatingCircuitMonitor` is out of scope.

Hardware: all existing ESP32-S3 boards stay in service for now. All new nodes use ESP32-C6. The S3 and the C6 are the only supported chips.

Priorities, in order:

1. Little LoRa airtime.
2. Simplicity: every packet carries the node's complete dataset, and messages are never split.
3. Low power on battery nodes.
4. Correct data, even where that means not copying legacy bugs.

## Where to look

- `ARCHITECTURE.md` covers the repo layout, design rules, build/test commands and the LoRa protocol. Read it before changing code, and update it when a decision changes.
- `docs/legacy-porting.md` covers the behaviour, register maps, known bugs and pin maps taken from the legacy projects. Delete each section once its port has landed.
