//! `deskcheck` command line tool.
//!
//! For now it only prints its version. The real commands come with the next
//! tasks (see `KANBAN.md`).

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        None | Some("--version" | "-V") => {
            println!("{}", deskcheck_core::version_line());
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("deskcheck: unknown argument `{other}`");
            eprintln!("Usage: deskcheck [--version]");
            ExitCode::from(2)
        }
    }
}
