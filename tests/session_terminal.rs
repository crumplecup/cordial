//! Finding the shell of the terminal a command runs under.

use cordial::host::{ProcessEntry, owner_of_chain};

fn entry(pid: u32, parent: u32, session: u32, terminal: u32) -> ProcessEntry {
    ProcessEntry {
        pid,
        parent,
        session,
        terminal,
    }
}

#[test]
fn a_command_typed_in_a_terminal_belongs_to_that_terminals_shell() {
    cordial::init_tracing();
    // cordial, then the shell that leads the terminal's session.
    let chain = [entry(500, 400, 400, 34821), entry(400, 90, 400, 34821)];
    assert_eq!(owner_of_chain(&chain), Some(400));
}

#[test]
fn a_command_run_by_a_tool_belongs_to_the_terminal_the_tool_runs_in() {
    cordial::init_tracing();
    // The command's own shell leads a session with no terminal; the tool above it
    // has one, and its session leader is the user's shell.
    let chain = [
        entry(900, 800, 800, 0),
        entry(800, 700, 800, 0),
        entry(700, 400, 400, 34821),
        entry(400, 90, 400, 34821),
    ];
    assert_eq!(owner_of_chain(&chain), Some(400));
}

#[test]
fn a_command_with_no_terminal_anywhere_has_no_owner() {
    cordial::init_tracing();
    let chain = [entry(900, 800, 800, 0), entry(800, 1, 800, 0)];
    assert_eq!(owner_of_chain(&chain), None);
    assert_eq!(owner_of_chain(&[]), None);
}
