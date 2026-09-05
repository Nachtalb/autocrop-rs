//! `autocrop` command line: crop files or folders.

use std::process::ExitCode;

fn main() -> ExitCode {
    let code = autocrop::cli::run(std::env::args().skip(1));
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
