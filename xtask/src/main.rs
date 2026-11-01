//! Build tool for this repo. Run it with `cargo xtask <command>`.

mod config;
mod firmware;
mod mqtt;
mod schemas;
mod validate;

use std::process::ExitCode;

const HELP: &str = "\
Usage: cargo xtask <command>

Commands:
  build <node>                 validate, then build the firmware of a node from nodes.toml
  build --all                  validate, then build the firmware of every node (CI does this)
  clippy <node>                validate, then run clippy on the firmware of a node
  run <node> [--port COM5]     build, publish schemas.json (warns if that fails), flash, show the log
  monitor <node> [--port COM5] show the log of a node that is already flashed
  check                        load boards.toml and nodes.toml, print them and validate them
  schemas                      validate, then write schemas.json for the decoder
  schemas --publish            ... and publish it retained to aio/schemas
  help                         show this help

Without --port, espflash looks for the board itself.
The MQTT broker comes from AIO_MQTT_URL, for example mqtt://user:pass@192.168.1.10:1883.
";

fn main() -> ExitCode {
    // The first argument is the program itself, so skip it.
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Borrow them as &str, so `match` can compare them with text like "build".
    let args: Vec<&str> = args.iter().map(String::as_str).collect();

    let result = match args.as_slice() {
        // Before `["build", node]`, which would take `--all` for a node name.
        ["build", "--all"] => firmware::build_all(),
        ["build", node] => firmware::build(node).map(|elf| println!("ELF: {}", elf.display())),
        ["clippy", node] => firmware::clippy(node),
        ["run", node, options @ ..] => {
            port_option(options).and_then(|port| firmware::run(node, port))
        }
        ["monitor", node, options @ ..] => {
            port_option(options).and_then(|port| firmware::monitor(node, port))
        }
        ["check"] => check(),
        ["schemas"] => schemas(false),
        ["schemas", "--publish"] => schemas(true),
        ["help" | "-h" | "--help"] => {
            print!("{HELP}");
            return ExitCode::SUCCESS;
        }
        [command @ ("build" | "clippy" | "run" | "monitor")] => Err(format!(
            "`{command}` needs a node, for example `cargo xtask {command} dev-c6`"
        )),
        [] => {
            eprint!("{HELP}");
            return ExitCode::FAILURE;
        }
        _ => {
            eprintln!("error: unknown command `{}`\n", args.join(" "));
            eprint!("{HELP}");
            return ExitCode::FAILURE;
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Reads the options after the node: either nothing or `--port <port>`.
fn port_option<'a>(options: &[&'a str]) -> Result<Option<&'a str>, String> {
    match options {
        [] => Ok(None),
        ["--port", port] => Ok(Some(port)),
        _ => Err(format!(
            "unknown options `{}`; the only option is `--port <port>`",
            options.join(" ")
        )),
    }
}

/// Validates the config files, then writes `schemas.json` and, with
/// `publish`, publishes it.
fn schemas(publish: bool) -> Result<(), String> {
    let boards = config::load_boards()?;
    let nodes = config::load_nodes()?;
    validate::run(&boards, &nodes)?;
    let text = schemas::write(&nodes)?;
    if publish {
        mqtt::publish_schemas(&text)?;
    }
    Ok(())
}

/// Loads the config files, prints what it found and validates it.
fn check() -> Result<(), String> {
    let boards = config::load_boards()?;
    let nodes = config::load_nodes()?;

    println!("boards.toml: {} boards", boards.len());
    for (name, board) in &boards {
        print!("  {name}: {:?}, {:?}", board.chip, board.radio);
        if let Some(mv) = board.tcxo_mv {
            print!(", TCXO {mv} mV");
        }
        println!();
        print!("    lora:");
        for (pin, gpio) in board.lora.pins() {
            print!(" {pin}={gpio}");
        }
        println!();
        if let Some(battery) = &board.battery {
            println!(
                "    battery: adc={}, divider {}",
                battery.adc, battery.divider
            );
        }
    }

    println!("nodes.toml: {} nodes", nodes.nodes.len());
    print!("  reserved_node_ids:");
    for id in &nodes.reserved_node_ids {
        print!(" {id:#04x}");
    }
    println!();
    for (name, node) in &nodes.nodes {
        println!(
            "  {name}: board {}, node_id {}, {:?}, heartbeat {} s, min_interval {} s",
            node.board, node.node_id, node.power, node.heartbeat, node.min_interval
        );
        let sources: Vec<_> = node.sources.keys().collect();
        println!("    sources: {sources:?}");
    }
    if nodes.devices.is_empty() {
        println!("  devices: none, so each node is one HA device");
    }
    for (key, device) in &nodes.devices {
        println!(
            "  device {key}: {:?}, sources {:?}",
            device.name, device.sources
        );
    }

    validate::run(&boards, &nodes)
}
