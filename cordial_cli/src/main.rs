//! CLI entry point for `cordial`.

mod boundary;

fn main() {
    if let Err(report) = boundary::run() {
        boundary::exit_on_error(report);
    }
}
