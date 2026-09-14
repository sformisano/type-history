//! Cargo subcommand entry point for standalone field histories.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::error::Error;

fn main() {
    if let Err(error) = cargo_type_history::run(std::env::args_os().skip(1)) {
        eprintln!("{}", render_error(&error));
        std::process::exit(2);
    }
}

fn render_error(error: &(dyn Error + 'static)) -> String {
    let mut previous = error.to_string();
    let mut rendered = format!("cargo-type-history: {previous}");
    let mut source = error.source();
    while let Some(cause) = source {
        let message = cause.to_string();
        if message != previous {
            rendered.push_str("\n  caused by: ");
            rendered.push_str(&message);
        }
        previous = message;
        source = cause.source();
    }
    rendered
}
