//! Build tool for this repo. Run it with `cargo xtask <command>`.

mod config;
mod firmware;
mod validate;

use std::process::ExitCode;

const HELP: &str = "\
Usage: cargo xtask <command>

Commands:
  build <node>                 validate, then build the firmware of a node from nodes.toml
  clippy <node>                validate, then run clippy on the firmware of a node
  run <node> [--port COM5]     build, flash and show the log
  monitor <node> [--port COM5] show the log of a node that is already flashed
  check                        load boards.toml and nodes.toml, print them and validate them
  help                         show this help

Without --port, espflash looks for the board itself.
";

fn main() -> ExitCode {
    // The first argument is the program itself, so skip it.
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Borrow them as &str, so `match` can compare them with text like "build".
    let args: Vec<&str> = args.iter().map(String::as_str).collect();

    let result = match args.as_slice() {
        ["build", node] => firmware::build(node).map(|elf| println!("ELF: {}", elf.display())),
        ["clippy", node] => firmware::clippy(node),
        ["run", node, options @ ..] => {
            port_option(options).and_then(|port| firmware::run(node, port))
        }
        ["monitor", node, options @ ..] => {
            port_option(options).and_then(|port| firmware::monitor(node, port))
        }
        ["check"] => check(),
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
    }

    validate::run(&boards, &nodes)
}
