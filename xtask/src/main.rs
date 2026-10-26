//! Build tool for this repo. Run it with `cargo xtask <command>`.

mod config;

use std::process::ExitCode;

const HELP: &str = "\
Usage: cargo xtask <command>

Commands:
  check   load boards.toml and print it
  help    show this help
";

fn main() -> ExitCode {
    // The first argument is the program itself, so the command is the second.
    let command = std::env::args().nth(1);

    match command.as_deref() {
        Some("check") => match check() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
        Some("help" | "-h" | "--help") => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("error: unknown command `{other}`\n");
            eprint!("{HELP}");
            ExitCode::FAILURE
        }
        None => {
            eprint!("{HELP}");
            ExitCode::FAILURE
        }
    }
}

/// Loads the config files and prints what it found.
fn check() -> Result<(), String> {
    let boards = config::load_boards()?;

    println!("boards.toml: {} boards", boards.len());
    for (name, board) in &boards {
        println!("  {name}: {:?}, {:?}", board.chip, board.radio);
        print!("    lora:");
        for (pin, gpio) in board.lora.pins() {
            print!(" {pin}={gpio}");
        }
        println!();
    }
    Ok(())
}
