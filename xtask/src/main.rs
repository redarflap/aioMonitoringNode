//! Build tool for this repo. Run it with `cargo xtask <command>`.

use std::process::ExitCode;

const HELP: &str = "\
Usage: cargo xtask <command>

Commands:
  help    show this help
";

fn main() -> ExitCode {
    // The first argument is the program itself, so the command is the second.
    let command = std::env::args().nth(1);

    match command.as_deref() {
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
