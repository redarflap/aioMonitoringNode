# Scaffold plan

This plan builds the empty frame of the project (the "scaffold") in small steps. Each step adds one thing, ends with a **Check**, and gets its own commit. Do the steps in order.

`ARCHITECTURE.md` describes the target design. Read it once before you start. When the real code ends up different, update `ARCHITECTURE.md` in the same commit.

## When the scaffold is done

- `cargo test` runs the host tests of all library crates.
- `cargo xtask run <node>` builds, flashes and monitors a node, on an ESP32-C6 and on an ESP32-S3.
- The node takes its node ID, pins and sources from `nodes.toml` and `boards.toml`.
- The node sends a test LoRa packet, and the OpenMQTTGateway receives it.
- CI on GitHub runs the host checks and builds the firmware.

Not part of the scaffold: the real sources (DS18B20, battery, Eder, Pyromat), the packet format, the send policy, deep sleep, the LP core and the decoder. [What comes next](#what-comes-next) lists them.

## Words you will meet

- **Host:** your PC. **Target:** the chip the code runs on.
- **Crate:** one Rust package, a library or a program. **Workspace:** several crates that share one `Cargo.lock` and one `target/` folder.
- **`no_std`:** code without Rust's standard library, because the chip has no operating system. It can still use `core`, the basic part of Rust.
- **Target triple:** the compiler's name for a CPU type. C6: `riscv32imac-unknown-none-elf`. S3: `xtensa-esp32s3-none-elf`.
- **Feature:** a switch in `Cargo.toml` that turns code on or off at compile time, for example `esp32c6`.
- **esp-hal:** the library that drives the chip's hardware (pins, SPI, UART, …).
- **Embassy:** lets you write `async` tasks that run side by side. `esp-rtos` runs them on the ESP32.
- **xtask:** a normal Rust program in this repo that runs the build for you. You call it with `cargo xtask …`.
- **defmt:** a small, fast logging library. **espflash:** flashes the chip and shows its log.

## How to work through this plan

- One step at a time. Don't start the next step until the check passes.
- Commit after each step, including `Cargo.lock`.
- If you get stuck, stop and ask. Don't skip a step.
- Versions change fast in embedded Rust, so this plan doesn't name any. Step 1.2 shows how to get the current ones.

---

## Phase 0: Tools

### 0.1 Install Rust and the C6 target

```sh
rustup update stable
rustup target add riscv32imac-unknown-none-elf
```

**Check:** `rustc --version` prints a version.

### 0.2 Install the ESP tools

```sh
cargo install espflash --locked       # flashes the chip, shows the log
cargo install esp-generate --locked   # creates new esp-hal projects
cargo install espup --locked          # installs the S3 toolchain (next step)
```

**Check:** `espflash --version` and `esp-generate --version` both work.

### 0.3 Install the S3 toolchain

The S3 has an Xtensa CPU. Normal Rust can't compile for it, so Espressif ships its own Rust toolchain, called `esp`.

```sh
espup install
```

On Windows, espup sets the environment variables for you. Open a new terminal afterwards.

**Check:** `rustup toolchain list` shows `esp`.

### 0.4 Connect a board

Plug a dev board in by USB. Always use a spare board, never one that is deployed.

```sh
espflash board-info
```

**Check:** it prints the chip (`esp32c6` or `esp32s3`). If it finds no port, the board may need a USB driver (CP210x or CH340), or you may have to hold BOOT while you plug it in.

---

## Phase 1: Hello world, outside the repo

This phase is for learning and for getting settings that work. You throw this project away at the end.

### 1.1 Generate a project

In a folder outside this repo:

```sh
esp-generate --headless -o esp32c6 -o unstable-hal -o embassy -o defmt -o esp-backtrace hello-c6
```

`--headless` skips the menu. Each `-o` turns on one option: the chip, Embassy (which needs `unstable-hal`), defmt logging, and esp-backtrace for panics. `esp-generate list-options` shows all options.

If you only have an S3 board, use `-o esp32s3`, and read "S3" wherever this plan says "C6" until Phase 5.

**Check:** the folder `hello-c6` has a `Cargo.toml`, a `build.rs`, a `.cargo/config.toml` and a `src/` folder.

### 1.2 Flash it and note the versions

```sh
cd hello-c6
cargo run --release
```

**Check:** the log prints a line every second. Stop it with Ctrl+C.

Keep these files open. Later steps copy from them:

- `Cargo.toml`: the versions of the `esp-*`, `embassy-*` and `defmt` crates, and the `[profile.*]` sections.
- `.cargo/config.toml`: the `rustflags`. It `include`s `.cargo/esp-config.toml`, which holds the `[env]` section.
- `build.rs`: the linker settings.
- `src/`: how esp-hal and esp-rtos get started.

No commit: this project isn't part of the repo.

---

## Phase 2: Workspace and libraries

### 2.1 The workspace and `crates/proto`

1. Create the first library:

   ```sh
   cargo new --lib --vcs none crates/proto
   ```

   `--vcs none` stops cargo from creating a second git repo (`.git`, `.gitignore`) inside this one.

2. Create `Cargo.toml` in the repo root:

   ```toml
   [workspace]
   resolver = "3"
   members = ["crates/proto"]
   default-members = ["crates/proto"]   # what a plain `cargo test` covers

   [workspace.package]
   edition = "2024"

   [workspace.dependencies]
   # Shared crate versions go here, so every crate uses the same ones.

   # Paste the [profile.*] sections from hello-c6/Cargo.toml here.
   # Release must keep opt-level = "s".
   ```

3. In `crates/proto/Cargo.toml`, replace the `edition` line with `edition.workspace = true`.

4. At the top of `crates/proto/src/lib.rs`, add:

   ```rust
   //! LoRa packet format and schema registry.
   #![cfg_attr(not(test), no_std)]
   ```

   This means: `no_std` on the chip, but tests on your PC may use `std` (for example `println!`). Keep the generated `add` function and its test for now, so there is something to run.

5. Create `rust-toolchain.toml` in the root, so everyone uses the same compiler setup:

   ```toml
   [toolchain]
   channel = "stable"
   components = ["rustfmt", "clippy"]
   targets = ["riscv32imac-unknown-none-elf"]
   ```

6. Create `.cargo/config.toml` in the root. It sets no build target on purpose, so `cargo test` runs on your PC:

   ```toml
   [alias]
   xtask = "run -p xtask --"
   ```

7. Create `.gitignore` with the line `/target`.

**Check:**

```sh
cargo test
cargo clippy --all-targets
cargo fmt --check
cargo build -p proto --target riscv32imac-unknown-none-elf
```

All four pass. The last one proves the crate really is `no_std`: host tests can pass even when a dependency secretly needs `std`.

Commit: `Add workspace and proto crate`

### 2.2 Empty library crates

Create `policy`, `onewire`, `lp-shared`, `eder` and `pyromat` in `crates/` the same way. Each `lib.rs` gets only the `no_std` line and a one-line doc comment saying what goes there (take it from the repo layout in `ARCHITECTURE.md`). Delete their generated `add` function. `cargo new` adds each crate to `members` for you; add it to `default-members` yourself.

**Check:** `cargo test` and `cargo clippy --all-targets` pass. `cargo build --target riscv32imac-unknown-none-elf` builds all six crates.

Commit: `Add empty library crates`

---

## Phase 3: The xtask

### 3.1 xtask skeleton

```sh
cargo new --vcs none xtask
```

Add `xtask` to `default-members` (`cargo new` already put it in `members`), and set `edition.workspace = true`. The xtask runs on your PC, so it can use `std` and normal crates.

In `xtask/src/main.rs`, read the command with `std::env::args()` and `match` on it. Start with a `help` command that lists the commands. An unknown command prints the help and exits with code 1.

**Check:** `cargo xtask help` prints the help. `cargo xtask foo` prints it too and fails.

From now on, a plain `cargo build --target riscv32imac-unknown-none-elf` fails, because it also builds the xtask, which needs `std`. Name the libraries instead: `-p proto -p policy -p onewire -p lp-shared -p eder -p pyromat`.

Commit: `Add xtask skeleton`

### 3.2 `boards.toml`

Create `boards.toml` in the root, with only the LoRa pins for now. Each source adds its own pins (1-Wire, UART, …) when it gets ported.

You decide a device's pins right before you flash it. Until then, its pins are placeholders. Changing them later is an edit to `boards.toml`, never to Rust code.

```toml
[boards.c6-dev]                  # placeholder pins; set the real ones before you flash
chip = "esp32c6"
radio = "sx126x"
lora = { sck = 6, mosi = 7, miso = 2, cs = 18, rst = 19, irq = 20, busy = 21 }

[boards.t3s3]                    # from docs/legacy-porting.md
chip = "esp32s3"
radio = "sx126x"
lora = { sck = 5, mosi = 6, miso = 3, cs = 7, rst = 8, irq = 33, busy = 34 }
```

Add the `feather-s3` and the pyromat board when you get ready to flash one of them.

In the xtask:

1. `cargo add -p xtask serde --features derive` and `cargo add -p xtask toml`.
2. Write structs that match the file, with `#[derive(Deserialize)]`. Make `chip` and `radio` enums. Add `#[serde(deny_unknown_fields)]`, so a typo in the TOML becomes an error.
3. Add a `check` command that loads `boards.toml` and prints it.

**Check:** `cargo xtask check` prints the boards. Put a typo into the file and `cargo xtask check` names it.

Commit: `Add boards.toml`

### 3.3 `nodes.toml`

Create `nodes.toml` with the real `reserved_node_ids` from `ARCHITECTURE.md` and one dev node:

```toml
# First bytes that legacy nodes still send. Remove each once its node is reflashed.
reserved_node_ids = [0x00, 0x01, 0x02, 0x7C]

[nodes.dev-c6]
board = "c6-dev"
node_id = 100
power = "always-on"
heartbeat = 60      # s, short for testing
min_interval = 10   # s
```

Parse it in the xtask like `boards.toml`. Make `power` an enum; `#[serde(rename_all = "kebab-case")]` maps `always-on` to `AlwaysOn`. `sources` comes in step 7.2.

**Check:** `cargo xtask check` also prints the nodes.

Commit: `Add nodes.toml`

### 3.4 First validation rules

Add these rules to `cargo xtask check`. Write each one as a small function with a unit test that uses a short TOML string, not the real files.

- Every node's `board` exists.
- Every `node_id` is unique and not in `reserved_node_ids`.
- Every pin exists on its chip: C6 has GPIO 0–30, S3 has GPIO 0–21 and 26–48.
- No board uses a pin twice.
- No board uses a flash pin: C6 GPIO 24–30, S3 GPIO 26–32.
- Warn, but don't fail, on strapping pins: C6 GPIO 4, 5, 8, 9, 15; S3 GPIO 0, 3, 45, 46.
- `busy` is set on `sx126x` boards and missing on `sx127x` boards.

S3 boards with octal PSRAM also lose GPIO 33–37. The feather-s3 and t3s3 use those pins for LoRa, so don't forbid them. The other rules in `ARCHITECTURE.md` (1-Wire pins, packet size, HA devices, …) arrive together with the code they protect.

**Check:** `cargo test -p xtask` passes. Give a second node the same `node_id`, and `cargo xtask check` fails with a clear message.

Commit: `Validate boards and nodes`

---

## Phase 4: Firmware on the C6

### 4.1 Move hello world into `firmware/node`

1. Copy `Cargo.toml`, `build.rs`, `.clippy.toml`, `src/` and `.cargo/` (both `config.toml` and `esp-config.toml`) from `hello-c6` into `firmware/node`. Don't copy `target/`, `Cargo.lock`, `rust-toolchain.toml`, `.gitignore` or `.vscode/`.
2. In `firmware/node/Cargo.toml`:
   - Rename the package to `node`, and the `[[bin]]` too if there is one. Set `edition.workspace = true`.
   - Delete the `[profile.*]` sections. Profiles only work in the root `Cargo.toml`.
   - Move each dependency's version into `[workspace.dependencies]` in the root, and write `esp-hal = { workspace = true, features = [...] }` here. Pin the `esp-*` versions exactly, like `"=1.2.3"`. Their unstable APIs can break on minor updates.
3. Add `firmware/node` to `members` only, **not** to `default-members`. That way `cargo test` never tries to build firmware for your PC.
4. Edit `firmware/node/.cargo/config.toml`:
   - Keep the `include` of `esp-config.toml`.
   - Delete the `runner` line and put the `rustflags` from `[build]` in its place, under `[target.riscv32imac-unknown-none-elf]`. Each chip then has its own flags; the S3 ones follow in step 5.1. (Once a `[target.…]` table has `rustflags`, cargo ignores the ones in `[build]` for that target.)
   - Delete what is left of `[build]` (`target = …`) and the `[unstable]` table. The xtask picks the target, stable Rust ignores `[unstable]` anyway, and the S3 build gets `build-std` from the xtask in step 5.1.

   The file then looks like this:

   ```toml
   include = ["esp-config.toml"]

   [target.riscv32imac-unknown-none-elf]
   rustflags = [
     # Required to obtain backtraces (e.g. when using the "esp-backtrace" crate.)
     "-C", "force-frame-pointers",
   ]
   ```

   Cargo reads this file only when it runs inside `firmware/node`.
5. Build and flash by hand, from inside `firmware/node`:

   ```sh
   cargo build --release --target riscv32imac-unknown-none-elf
   espflash flash --monitor --log-format defmt ../../target/riscv32imac-unknown-none-elf/release/node
   ```

**Check:** you see the same log as in step 1.2. In the root, `cargo test` still runs only the host crates.

Commit: `Add firmware/node from the esp-generate template`

### 4.2 Chip features

The chip name is still written into the esp-* dependency lines. Make it a feature instead, so the same crate builds for both chips:

```toml
[features]
esp32c6 = ["esp-hal/esp32c6", "esp-rtos/esp32c6", "esp-backtrace/esp32c6", "esp-println/esp32c6", "esp-bootloader-esp-idf/esp32c6"]
esp32s3 = ["esp-hal/esp32s3", "esp-rtos/esp32s3", "esp-backtrace/esp32s3", "esp-println/esp32s3", "esp-bootloader-esp-idf/esp32s3"]
```

List every esp-* crate that had a chip feature in hello-c6, and remove the chip from its dependency line.

**Check:** `cargo build --release --target riscv32imac-unknown-none-elf --features esp32c6` builds. Without `--features`, esp-hal stops with an error that asks for a chip.

Commit: `Select the chip with a cargo feature`

### 4.3 `cargo xtask build` and `clippy`

The xtask now does what you typed by hand:

1. Run the checks from step 3.4, and stop on errors.
2. Find the node's board, and from it the chip.
3. Pick the target: `esp32c6` → `riscv32imac-unknown-none-elf`.
4. Run `cargo build --release --target <target> --features <chip>` with `std::process::Command`, with `.current_dir("firmware/node")`.
5. Print the path of the ELF file.

`cargo xtask clippy <node>` is the same, with `clippy` instead of `build`.

**Check:** `cargo xtask build dev-c6` builds and prints the ELF path. `cargo xtask build nope` says the node doesn't exist. `cargo xtask clippy dev-c6` runs.

Commit: `Add xtask build and clippy`

### 4.4 `cargo xtask run` and `monitor`

- `run <node> [--port COM5]`: build, then `espflash flash --monitor --log-format defmt [--port …] <elf>`.
- `monitor <node> [--port …]`: `espflash monitor --log-format defmt --elf <elf>`. It needs the ELF to decode the defmt log.

Publishing `schemas.json` before flashing comes later, with the decoder.

**Check:** `cargo xtask run dev-c6` flashes and shows the log. Close it, run `cargo xtask monitor dev-c6`, press the board's reset button, and the log is readable.

Commit: `Add xtask run and monitor`

### 4.5 Node values from `nodes.toml`

The firmware never hard-codes node values. The xtask passes them as environment variables, and the firmware reads them at compile time with `env!`.

1. xtask: add `.env("AIO_NODE_ID", …)`, `AIO_HEARTBEAT` and `AIO_MIN_INTERVAL` to the build command.
2. firmware: create `src/config.rs`:

   ```rust
   //! Values from nodes.toml and boards.toml, passed in by the xtask at build time.

   const fn parse_u8(s: &str) -> u8 {
       match u8::from_str_radix(s, 10) {
           Ok(v) => v,
           Err(_) => panic!("expected a number from 0 to 255"),
       }
   }

   const fn parse_u32(s: &str) -> u32 {
       match u32::from_str_radix(s, 10) {
           Ok(v) => v,
           Err(_) => panic!("expected a number"),
       }
   }

   pub const NODE_ID: u8 = parse_u8(env!("AIO_NODE_ID"));
   pub const HEARTBEAT_S: u32 = parse_u32(env!("AIO_HEARTBEAT"));
   pub const MIN_INTERVAL_S: u32 = parse_u32(env!("AIO_MIN_INTERVAL"));
   ```

   These are `const`, so the compiler computes them. A bad value fails the build instead of crashing the chip.
3. Log the values at start-up.

From now on, build only with `cargo xtask`. A hand-typed `cargo build` doesn't set these variables.

**Check:** the log shows `node_id` 100. Change it in `nodes.toml`, run again, and the log shows the new value.

Commit: `Pass node values to the firmware`

### 4.6 Pin numbers from `boards.toml`

Pins travel the same way. The xtask exports each pin of the node's board as `AIO_PIN_<GROUP>_<NAME>`, for example `AIO_PIN_LORA_SCK=6`. It doesn't export pins the board doesn't have.

In `config.rs`, read required pins with `env!`, and optional ones (like `busy`) with `option_env!`:

```rust
pub const LORA_SCK: u8 = parse_u8(env!("AIO_PIN_LORA_SCK"));
pub const LORA_BUSY: Option<u8> = match option_env!("AIO_PIN_LORA_BUSY") {
    Some(s) => Some(parse_u8(s)),
    None => None,
};
```

Log the LoRa pins at start-up. Nothing uses them yet; that happens in Phase 6.

**Check:** the log shows the pins from `boards.toml`. Change one, run again, and the log follows.

Commit: `Pass board pins to the firmware`

---

## Phase 5: S3 support

### 5.1 Build for the S3

1. Make a second throwaway project with `esp-generate --headless -o esp32s3 -o unstable-hal -o embassy -o defmt -o esp-backtrace hello-s3`. Copy its `rustflags` into `firmware/node/.cargo/config.toml`, under `[target.xtensa-esp32s3-none-elf]`.
2. In the xtask, for an S3 node:
   - use the target `xtensa-esp32s3-none-elf`,
   - run `cargo +esp …` instead of `cargo …`,
   - add `-Zbuild-std=core` (the Rust core library has to be compiled for Xtensa). Use the same list as the `[unstable] build-std` line in `hello-s3/.cargo/config.toml`.

   Start `cargo` by name. Don't use the `CARGO` environment variable that `cargo xtask` sets: it points straight to the stable cargo, which doesn't understand `+esp`.

   Keep this S3-only code together in one place, so it's easy to delete later.
3. Add a node `dev-s3` on board `t3s3` (or your own S3 board) to `nodes.toml`.

**Check:** `cargo xtask build dev-s3` builds. The first S3 build is slow, because it compiles `core`.

Commit: `Build S3 nodes`

### 5.2 Run on an S3

Compare `hello-s3/src` with `hello-c6/src`. Where the start-up code differs, put each version behind `#[cfg(feature = "esp32c6")]` or `#[cfg(feature = "esp32s3")]`.

**Check:** `cargo xtask run dev-s3` shows the log on a spare S3 board, with dev-s3's node ID. `cargo xtask run dev-c6` still works.

Commit: `Run on the S3`

**Status (2026-10-27):** `hello-s3/src` and `hello-c6/src` are the same, so `main.rs` needs no `#[cfg]`. The only start-up difference was the `-Wl,` linker option in `build.rs`, which step 5.1 already needed. `espflash save-image --chip esp32s3` accepts the dev-s3 ELF, and the dev-c6 flash image has the same code bytes as before Phase 5. `cargo xtask run dev-s3` works on a real S3 board.

---

## Phase 6: LoRa

### 6.1 Board module and radio driver

1. firmware: add the features `sx126x` and `sx127x`. The xtask picks one from the board's `radio`.
2. Add `lora-phy` and `embedded-hal-bus` to `[workspace.dependencies]`, and use them in `firmware/node`. `embedded-hal-bus` joins the SPI bus and the CS pin into one "SPI device", which `lora-phy` needs.
3. Create `src/board/mod.rs` with `pub fn init(peripherals) -> Board`, plus one file per chip: `src/board/esp32c6.rs` and `src/board/esp32s3.rs`.
   - `init` is the **only** code that turns pin numbers into esp-hal pins. esp-hal has an `unsafe` way to make a pin from a number (look for `steal` on `AnyPin` in the esp-hal docs). It's safe here, because the xtask has already checked that no pin is used twice.
   - `Board` hands out ready-made drivers, so the rest of the code never sees a pin number. For now that is only the radio.
4. Create `src/radio.rs` with the radio settings from "Radio" in `ARCHITECTURE.md`: 868.0 MHz, SF7, BW 125 kHz, CR 4/5, preamble 8, CRC on, 14 dBm. Create the `lora-phy` `LoRa` object with `public_network = false`.
   - `sx126x`: DIO2 as RF switch, TCXO on DIO3, LDO regulator.
   - `sx127x`: PA_BOOST.
5. The c6-dev board has no `irq` pin (its DIO0 isn't wired), so it needs the software stand-in described under "Radio" in `ARCHITECTURE.md`. Put it in `src/radio.rs`:
   - Add `embassy-sync` and `embedded-hal-async` to the dependencies.
   - Put the SPI device into an `embassy_sync::mutex::Mutex`. Give `lora-phy` a small wrapper around it that implements `embedded_hal_async::spi::SpiDevice` by locking the mutex for each transaction.
   - Write a type that implements `embedded_hal_async::digital::Wait`; `lora-phy` only calls `wait_for_high`. It locks the same mutex and reads register 0x12: send the bytes `[0x12, 0x00]`, and the second byte that comes back is the value. It repeats this every 1 ms until the value isn't 0. Pass it to `GenericSx127xInterfaceVariant` where the IRQ pin would go.
   - When a board has an `irq` pin, `board::init` passes the real pin instead, and nothing else changes.
6. The TCXO voltage is a board setting. Add `tcxo_mv = 1800` to the t3s3 in `boards.toml`; boards without a TCXO leave it out. The xtask exports it like a pin. `cargo xtask check` accepts it only on `sx126x` boards and only with a value `lora-phy` supports (1600, 1700, 1800, 2200, 2400, 2700, 3000, 3300).

The c6-dev pins come from its schematic. Before you flash the S3, check the t3s3 pins in `boards.toml` against your board.

**Check:** both boards log `radio ready`. With a wrong pin, start-up logs an error instead of hanging. If an SX126x radio doesn't start, try `tcxo_mv = 3300`, the value the legacy code used.

Commit: `Add board module and LoRa driver`

**Status (2026-10-27):** the code differs from the steps above in these places:

- `board::init` also starts esp-rtos. `esp_rtos::start` keeps its two peripherals forever, so `main` couldn't pass the rest of `Peripherals` on afterwards.
- There are no `src/board/esp32c6.rs` and `esp32s3.rs` yet, because nothing differs between the chips: both use SPI2, and any GPIO can be an SPI pin.
- `src/radio.rs` holds what both radio families share. The rest is in `src/radio/sx126x.rs` and `src/radio/sx127x.rs`; the SX127x stand-in for DIO0 is `Dio0::Poll`.
- The firmware also needs `embedded-hal`, where the digital `ErrorType` lives.
- Start-up reads back the sync word. Without that check, an SX127x with a wrong SPI or CS pin would still log `radio ready`, because `lora-phy` only writes to it while it starts.

### 6.2 Send a test packet

Add `send(bytes)` to `src/radio.rs`. In `main`, send `[NODE_ID, 0xAA, 0x55]` every `HEARTBEAT_S` seconds. These test bytes only live until `crates/proto` defines the real packet.

Wrap the wait for "sent" in `embassy_time::with_timeout` with about 2 s, and log an error when it runs out. Without it, a radio that never reports "done" would hang the node.

The xtask already keeps `node_id` out of `reserved_node_ids`, so the legacy Node-RED flow ignores these packets.

**Check:** watch the OpenMQTTGateway topic with MQTT Explorer. Every 60 s, a message with the bytes `64 AA 55` arrives from dev-c6 (100 = 0x64) and `65 AA 55` from dev-s3 (101 = 0x65).

Commit: `Send a LoRa test packet`

**Status (2026-10-27):** the 2 s timeout covers the whole send, not only the wait for "sent", because on SX126x preparing a send also waits on BUSY.

---

## Phase 7: Tasks

### 7.1 The report task

Move the sending into `src/report.rs`, as an Embassy task (`#[embassy_executor::task]`). `main` now only calls `board::init`, spawns the tasks and returns.

**Check:** the gateway still receives the test packet every 60 s.

Commit: `Move sending into the report task`

**Status (2026-10-27):** the report task also starts the radio, so `main` doesn't have to wait for it. If the radio doesn't start, the task logs the error and ends. On the dev-c6 board, the log shows `radio ready` from `report.rs` and then `sent [64, aa, 55]` every 60 s, and the gateway receives these packets.

### 7.2 Sources from `nodes.toml`

1. firmware: add one feature per source: `source-ds18b20`, `source-battery`, `source-eder`, `source-pyromat`.
2. Create `src/sources/mod.rs` with one `#[cfg(feature = "source-…")] mod …;` line per source, and a `spawn_all(spawner)` function that spawns the task of each enabled source.
3. Create `src/sources/<source>.rs` for all four. Each is a stub task that logs `<source>: not ported yet` and ends.
4. `nodes.toml`: add `sources.battery = {}` to `dev-c6`.
5. xtask: read `sources`, reject unknown names, and turn each source into `--features source-<name>`.

**Check:** the log shows `battery: not ported yet`. Remove the line from `nodes.toml`, run again, and the message is gone. `sources.foo = {}` makes `cargo xtask check` fail.

Commit: `Select sources from nodes.toml`

**Status (2026-10-27):** the xtask also rejects parameters that a source doesn't take; so far no source takes any. With all four sources on, `cargo xtask clippy` passes on both chips. On the dev-c6 board, the log shows `battery: not ported yet` and the message goes away without the line.

---

## Phase 8: Editor, CI and docs

### 8.1 rust-analyzer

Create `.vscode/settings.json`, so the editor can check the firmware:

```json
{
  "rust-analyzer.cargo.target": "riscv32imac-unknown-none-elf",
  "rust-analyzer.cargo.features": ["node/esp32c6", "node/sx126x", "node/source-battery", "node/source-ds18b20", "node/source-eder", "node/source-pyromat"],
  "rust-analyzer.cargo.extraEnv": {
    "AIO_NODE_ID": "100",
    "AIO_HEARTBEAT": "60",
    "AIO_MIN_INTERVAL": "10",
    "AIO_PIN_LORA_SCK": "6"
  },
  "rust-analyzer.check.allTargets": false
}
```

Add every `AIO_*` variable the firmware reads to `extraEnv`. Then run "rust-analyzer: Restart server" in VS Code.

rust-analyzer uses one target for the whole workspace. With the C6 target, the xtask shows false errors in the editor, because it needs `std`. `cargo xtask` still works. While you work on the xtask for longer, comment out the `cargo.target` line.

**Check:** in `firmware/node/src/main.rs`, hovering over an esp-hal type shows its docs, and there are no red errors.

Commit: `Configure rust-analyzer for the firmware`

### 8.2 CI on GitHub

1. xtask: add `cargo xtask build --all`, which builds every node in `nodes.toml`. It's handy locally, too.
2. Create `.github/workflows/ci.yml` with two jobs:
   - **host:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `cargo build --target riscv32imac-unknown-none-elf -p proto -p policy …` for the `no_std` libraries.
   - **firmware:** `cargo xtask check` and `cargo xtask build --all`. The S3 nodes need the `esp` toolchain; the `esp-rs/xtensa-toolchain` action installs it.

**Check:** push a branch and open a pull request. CI is green. Break the formatting on purpose, and CI turns red.

Commit: `Add CI`

### 8.3 Update the docs and clean up

- `ARCHITECTURE.md`: remove "Nothing is built yet". Add the new commands (`check`, `build --all`) and the `AIO_*` variable names, and fix anything that ended up different.
- Delete the throwaway `hello-c6` and `hello-s3` projects.
- Delete this plan, or mark it as done.

Commit: `Update docs after the scaffold`

---

## What comes next

Each item gets its own small plan later. Roughly in this order, simplest first:

1. **`battery` source** (always-on): the first real path from sensor to Home Assistant. Kind 4 in `crates/proto` with golden-byte tests, the snapshot-and-signal link between source task and report task, and a real packet instead of the test bytes. Confirm the kind's field types first; `ARCHITECTURE.md` marks them as proposed.
2. **Send policy** in `crates/policy`, with host tests.
3. **Decoder** in `decoder/` and `cargo xtask schemas`.
4. **`ds18b20`** on an always-on node, with `crates/onewire`.
5. **`eder`** and **`pyromat`** sources.
6. **Deep sleep** and the coprocessor (`lp-tempsense`, `lp-shared`) for the temp stations.
7. Reflash the legacy nodes one by one, and remove their IDs from `reserved_node_ids`.
