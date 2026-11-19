//! A run bounds its own memory: three quarters of what is available unless told otherwise, and
//! it says where it was when it stops itself.

mod support {
    pub mod mini_workspace;
}

use clap::Parser;
use cordial::{
    Backoff, Cli, DEFAULT_MEMORY_LIMIT, FIRST_GROWTH_KIB, FUTILE_KIB, MEMORY_LIMIT_EXIT,
    MemoryLimit, Pressure, WORKER_KIB, budget_of, budget_with_room, exceeds, footprint_of,
    functions_within, judge, judge_level, load_cordial_config, nearing, no_room, over_budget,
    parse_limit, parse_meminfo, parse_size, resolve_memory_limit_with, resolve_memory_target,
    workers_at, workers_within,
};
use miette::{IntoDiagnostic, Result};

const GIB: u64 = 1 << 30;

#[test]
fn sizes_are_binary_and_zero_is_no_limit() {
    assert_eq!(parse_size("8G"), Ok(8 * GIB));
    assert_eq!(parse_size("512m"), Ok(512 << 20));
    assert_eq!(parse_size("64K"), Ok(64 << 10));
    assert_eq!(parse_size("4096"), Ok(4096));
    assert_eq!(parse_size("0"), Ok(0));
    assert!(parse_size("lots").is_err());
    assert!(parse_size("").is_err());
    assert!(
        parse_size("99999999999T").is_err(),
        "an overflow is refused"
    );
}

#[test]
fn a_limit_is_a_size_or_a_share() {
    assert_eq!(parse_limit("12G"), Ok(MemoryLimit::Bytes(12 * GIB)));
    assert_eq!(parse_limit("75%"), Ok(MemoryLimit::Percent(75.0)));
    assert_eq!(parse_limit(" 12.5 % "), Ok(MemoryLimit::Percent(12.5)));
    assert_eq!(parse_limit("0%"), Ok(MemoryLimit::Percent(0.0)));
    assert!(parse_limit("101%").is_err(), "more than all of it");
    assert!(parse_limit("-5%").is_err());
    assert!(parse_limit("many%").is_err());
}

#[test]
fn the_memory_available_is_read_from_proc_meminfo() {
    let meminfo = "MemTotal:       32768000 kB\nMemFree:         1000000 kB\nMemAvailable:   16777216 kB\nBuffers: 1 kB\n";
    assert_eq!(parse_meminfo(meminfo), Some(16 * GIB));
    assert_eq!(parse_meminfo("MemTotal: 1 kB\n"), None);
}

#[test]
fn a_share_is_of_what_is_available_and_says_so() {
    let available = Some(16 * GIB);
    let budget = budget_of(MemoryLimit::Percent(75.0), available);
    assert_eq!(budget.bytes(), 12 * GIB);

    // Zero, in either form, is no limit.
    assert_eq!(budget_of(MemoryLimit::Percent(0.0), available).bytes(), 0);
    assert_eq!(budget_of(MemoryLimit::Bytes(0), available).bytes(), 0);

    // Where the available memory cannot be read a share cannot be worked out: a size that is
    // safe on a small machine is used instead of none.
    assert_eq!(budget_of(MemoryLimit::Percent(75.0), None).bytes(), 8 * GIB);
}

#[test]
fn a_process_is_over_a_limit_only_when_it_has_one() {
    let limit = GIB;
    assert!(!exceeds(1024 * 1024, limit), "1 GiB of 1 GiB is not over");
    assert!(exceeds(1024 * 1024 + 1, limit), "a kibibyte more is");
    assert!(!exceeds(u64::MAX, 0), "zero is no limit");
}

#[test]
fn a_process_eases_off_at_sixty_percent_of_its_limit_before_it_is_stopped() {
    let limit = GIB;
    let kib_at = |percent: u64| (GIB / 1024 * percent).div_ceil(100);
    assert!(!nearing(kib_at(59), limit), "59% is still working freely");
    assert!(
        nearing(kib_at(60), limit),
        "60% gives back what can be computed again"
    );
    assert!(!exceeds(kib_at(60), limit), "and is not yet over the limit");
    assert!(!nearing(u64::MAX, 0), "zero is no limit");
}

#[test]
fn pressure_begins_at_sixty_percent_and_ends_only_below_forty_five() {
    let limit = GIB;
    let kib_at = |percent: u64| (GIB / 1024 * percent).div_ceil(100);
    assert!(judge(false, kib_at(60), limit), "60% begins it");
    assert!(!judge(false, kib_at(59), limit), "59% does not");
    assert!(
        judge(true, kib_at(50), limit),
        "once begun, 50% does not end it"
    );
    assert!(judge(true, kib_at(45), limit), "nor does 45%");
    assert!(!judge(true, kib_at(44), limit), "44% does");
    assert!(!judge(true, u64::MAX, 0), "with no limit there is none");
}

#[test]
fn the_rungs_of_the_ladder_begin_at_sixty_seventy_five_and_eighty_five_percent() {
    let limit = GIB;
    let kib_at = |percent: u64| (GIB / 1024 * percent).div_ceil(100);
    let from_none = |percent: u64| judge_level(Pressure::None, kib_at(percent), limit);
    assert_eq!(from_none(59), Pressure::None);
    assert_eq!(from_none(60), Pressure::Soft);
    assert_eq!(from_none(75), Pressure::Hard);
    assert_eq!(from_none(85), Pressure::Severe);
    assert_eq!(from_none(99), Pressure::Severe);
    assert_eq!(
        judge_level(Pressure::Severe, u64::MAX, 0),
        Pressure::None,
        "no limit, no rung"
    );
}

#[test]
fn a_run_comes_down_a_rung_only_when_it_is_well_below_it() {
    let limit = GIB;
    let kib_at = |percent: u64| (GIB / 1024 * percent).div_ceil(100);
    // On the last rung 80% is not enough to leave it, 69% is, and then it is on the one below.
    assert_eq!(
        judge_level(Pressure::Severe, kib_at(80), limit),
        Pressure::Severe
    );
    assert_eq!(
        judge_level(Pressure::Severe, kib_at(69), limit),
        Pressure::Hard
    );
    // The second rung stays at 65% and is left below 60%, for the first; that is left below 45%.
    assert_eq!(
        judge_level(Pressure::Hard, kib_at(65), limit),
        Pressure::Hard
    );
    assert_eq!(
        judge_level(Pressure::Hard, kib_at(59), limit),
        Pressure::Soft
    );
    assert_eq!(
        judge_level(Pressure::Soft, kib_at(50), limit),
        Pressure::Soft
    );
    assert_eq!(
        judge_level(Pressure::Soft, kib_at(44), limit),
        Pressure::None
    );
    // It can drop several rungs at once if memory fell a long way.
    assert_eq!(
        judge_level(Pressure::Severe, kib_at(10), limit),
        Pressure::None
    );
}

#[test]
fn the_workers_are_as_many_as_fit_below_the_second_rung_and_at_least_one() {
    let kib = |workers: u64| i64::try_from(workers * WORKER_KIB).unwrap_or(i64::MAX);
    assert_eq!(workers_within(kib(5), 8), 5, "as many as fit");
    assert_eq!(
        workers_within(kib(5) + 1, 8),
        5,
        "a part of one does not make one"
    );
    assert_eq!(workers_within(kib(100), 8), 8, "never more than there are");
    assert_eq!(
        workers_within(kib(1) - 1, 8),
        1,
        "a run with none would not finish"
    );
    assert_eq!(workers_within(-500_000, 8), 1, "past the line, one");
    assert_eq!(
        workers_within(kib(3), 0),
        1,
        "and one if nothing was available to count"
    );
}

#[test]
fn the_costly_actions_wait_twice_as_long_after_each_round_that_gave_nothing_back() {
    let mut backoff = Backoff::new();
    assert!(backoff.act_now(), "the first round acts");
    backoff.gave_back(0);
    let skipped = |expected: u32, backoff: &mut Backoff| {
        for _ in 0..expected {
            assert!(!backoff.act_now(), "a round to skip");
        }
        assert!(
            backoff.act_now(),
            "then one that acts, after {expected} skipped"
        );
    };
    skipped(1, &mut backoff);
    backoff.gave_back(FUTILE_KIB - 1);
    skipped(3, &mut backoff);
    backoff.gave_back(0);
    skipped(7, &mut backoff);
    // However many in a row, it is not put off for longer than 63 rounds.
    for _ in 0..20 {
        backoff.gave_back(0);
        while !backoff.act_now() {}
    }
    backoff.gave_back(0);
    skipped(63, &mut backoff);
}

#[test]
fn a_round_that_gives_something_back_starts_the_waiting_again() {
    let mut backoff = Backoff::new();
    for _ in 0..4 {
        assert!(
            backoff.act_now() || {
                while !backoff.act_now() {}
                true
            }
        );
        backoff.gave_back(0);
    }
    assert!(
        !backoff.act_now(),
        "after rounds that did nothing, rounds are skipped"
    );
    while !backoff.act_now() {}
    backoff.gave_back(FUTILE_KIB);
    assert!(
        backoff.act_now(),
        "and what gave back enough starts again with a round that acts"
    );
    assert_eq!(backoff, Backoff::new());
}

#[test]
fn a_chunk_takes_as_many_functions_as_half_the_headroom_holds_at_the_growth_seen() {
    let mib = |mebibytes: i64| mebibytes * 1024;
    assert_eq!(
        functions_within(mib(400), FIRST_GROWTH_KIB, 134),
        50,
        "400 MB at the first growth taken, 4 MB a function, half of it"
    );
    assert_eq!(
        functions_within(mib(400), 400 * 1024, 134),
        1,
        "at least one"
    );
    assert_eq!(
        functions_within(mib(100_000), 1024, 134),
        134,
        "never more than the usual"
    );
    assert_eq!(
        functions_within(-mib(50), 4 * 1024, 134),
        1,
        "past the line, one"
    );
    assert_eq!(
        functions_within(mib(400), 0, 134),
        134,
        "a growth of nothing is not a limit"
    );
}

#[test]
fn the_ladder_works_to_a_target_below_the_limit_and_the_run_is_stopped_at_the_limit() {
    let available = Some(16 * GIB);
    let limit = resolve_memory_limit_with(None, None, available);
    assert_eq!(
        limit.ladder_bytes(),
        limit.bytes(),
        "no target: the ladder works to the limit"
    );

    let target = MemoryLimit::Bytes(1536 << 20);
    let low = resolve_memory_target(limit.clone(), Some(target), None, available);
    assert_eq!(low.bytes(), limit.bytes(), "the limit is as it was");
    assert_eq!(
        low.ladder_bytes(),
        1536 << 20,
        "the ladder works to the target"
    );

    let configured = resolve_memory_target(limit.clone(), None, Some("2G"), available);
    assert_eq!(
        configured.ladder_bytes(),
        2 * GIB,
        "cordial.toml gives it too"
    );

    let above = resolve_memory_target(
        limit.clone(),
        Some(MemoryLimit::Bytes(64 * GIB)),
        None,
        available,
    );
    assert_eq!(
        above.ladder_bytes(),
        limit.bytes(),
        "a target above the limit is the limit"
    );

    let share = resolve_memory_target(
        limit.clone(),
        Some(MemoryLimit::Percent(10.0)),
        None,
        available,
    );
    assert_eq!(share.bytes(), limit.bytes());
    assert!(
        share.ladder_bytes() < limit.bytes() && share.ladder_bytes() > 0,
        "a share works too"
    );

    let none = resolve_memory_target(limit.clone(), Some(MemoryLimit::Bytes(0)), None, available);
    assert_eq!(
        none.ladder_bytes(),
        limit.bytes(),
        "a target of none is no target"
    );
}

#[test]
fn a_run_is_over_its_budget_when_what_it_holds_or_what_the_machine_sees_of_it_is() {
    let limit = 2 * GIB;
    let kib = |bytes: u64| bytes / 1024;
    assert!(!over_budget(kib(GIB), kib(GIB), limit), "under both");
    assert!(
        over_budget(kib(3 * GIB), kib(GIB), limit),
        "holding too much"
    );
    assert!(
        over_budget(kib(GIB / 4), kib(3 * GIB), limit),
        "holding little, but the allocator keeps what the machine sees: over"
    );
    assert!(
        !over_budget(kib(100 * GIB), kib(100 * GIB), 0),
        "no limit is none"
    );
}

#[test]
fn the_rungs_allow_all_the_workers_then_a_quarter_then_one() {
    assert_eq!(workers_at(16, cordial::Pressure::None), 16);
    assert_eq!(workers_at(16, cordial::Pressure::Soft), 16);
    assert_eq!(workers_at(16, cordial::Pressure::Hard), 4);
    assert_eq!(workers_at(16, cordial::Pressure::Severe), 1);
    assert_eq!(workers_at(2, cordial::Pressure::Hard), 1, "never none");
}

#[test]
fn a_footprint_says_how_much_of_what_the_machine_saw_the_process_held() {
    // Held 400 MB at most, and when the machine saw 1.2 GB of it, 300 MB of that was held.
    let footprint = footprint_of(400 * 1024, 1200 * 1024, 300 * 1024);
    assert_eq!(footprint.peak_held_kib, 400 * 1024);
    assert_eq!(footprint.peak_resident_kib, 1200 * 1024);
    assert_eq!(
        footprint.kept_by_allocator_kib,
        900 * 1024,
        "what the machine saw that was not held"
    );
    assert_eq!(
        footprint.held_percent, 25,
        "a quarter of what was seen was held"
    );
    let nothing = footprint_of(0, 0, 0);
    assert_eq!(
        (nothing.kept_by_allocator_kib, nothing.held_percent),
        (0, 0)
    );
    let more_held = footprint_of(10, 5, 9);
    assert_eq!(more_held.kept_by_allocator_kib, 0, "never less than none");
}

#[test]
fn a_process_of_a_run_in_processes_spends_the_budget_less_what_the_driver_holds() {
    let limit = resolve_memory_limit_with(Some(MemoryLimit::Bytes(2 * GIB)), None, None);
    let budget = resolve_memory_target(limit, Some(MemoryLimit::Bytes(GIB)), None, None);
    let spent = budget.clone().reserving(16 << 20);
    assert_eq!(
        spent.bytes(),
        2 * GIB - (16 << 20),
        "the limit, less the driver's share"
    );
    assert_eq!(spent.ladder_bytes(), GIB - (16 << 20), "and the target");
    let tiny = budget.reserving(10 * GIB);
    assert_eq!(
        tiny.bytes(),
        1 << 20,
        "a driver that holds more than the budget leaves a mebibyte"
    );
    let none = resolve_memory_limit_with(Some(MemoryLimit::Bytes(0)), None, None).reserving(GIB);
    assert_eq!(none.bytes(), 0, "no limit stays none");
}

#[test]
fn a_process_with_no_room_below_the_checkpoint_line_is_seen_to_have_none() {
    let target = 1536 * 1024 * 1024_u64;
    // The line is 85% of the target: about 1.28 GB.
    assert!(
        no_room(1300 * 1024, target),
        "1.3 GB resident is past the line"
    );
    assert!(
        no_room(1250 * 1024, target),
        "and 1.25 GB is within 10% of it"
    );
    assert!(
        !no_room(800 * 1024, target),
        "800 MB leaves room to work in"
    );
    assert!(!no_room(1_000_000, 0), "no budget is none, and has room");
    // A budget that would have room for a floor of 1.3 GB is more than the 1.5 GB that did not.
    let needed = budget_with_room(1300 * 1024);
    assert!(needed > target, "{needed} against {target}");
    assert!(!no_room(1300 * 1024, needed), "and with it there is room");
}

#[test]
fn the_driver_projects_how_many_more_processes_a_run_needs_from_the_rate_they_go_at() {
    use cordial::{Progress, projected_processes};
    let working = |results: u64, remaining: u64| Progress::Working {
        results,
        remaining: Some(remaining),
        floor_kib: 1_300_000,
    };
    assert_eq!(projected_processes(&[]), None, "nothing yet");
    assert_eq!(
        projected_processes(&[working(64, 2000), working(128, 1936)]),
        None,
        "two are not enough to tell a rate from"
    );
    // 64 more each time, and 1,872 left: about thirty more processes.
    assert_eq!(
        projected_processes(&[working(64, 2000), working(128, 1936), working(192, 1872)]),
        Some(30)
    );
    // Faster, and what is left small: a few.
    assert_eq!(
        projected_processes(&[working(500, 1600), working(1000, 1100), working(1500, 600)]),
        Some(2)
    );
    // They got through nothing: not for this (the run is stuck, which is another check).
    assert_eq!(
        projected_processes(&[working(64, 100), working(64, 100), working(64, 100)]),
        None
    );
    // What is left is not known: no projection.
    let unknown = Progress::Working {
        results: 1,
        remaining: None,
        floor_kib: 0,
    };
    assert_eq!(projected_processes(&[unknown, unknown, unknown]), None);
}

#[test]
fn the_flag_beats_the_config_and_the_config_beats_the_default() -> Result<()> {
    assert_eq!(DEFAULT_MEMORY_LIMIT, "75%");
    let available = Some(32 * GIB);
    let resolve = |flag, configured| resolve_memory_limit_with(flag, configured, available).bytes();

    // Nothing says: three quarters of what is available.
    let none = Cli::try_parse_from(["cordial", "open"]).into_diagnostic()?;
    assert_eq!(none.memory_limit, None);
    assert_eq!(resolve(none.memory_limit, None), 24 * GIB);

    // The config says.
    assert_eq!(resolve(None, Some("12G")), 12 * GIB);
    assert_eq!(resolve(None, Some("50%")), 16 * GIB);
    assert_eq!(resolve(None, Some("0")), 0);

    // The flag says, before or after the subcommand, and beats the config.
    let before =
        Cli::try_parse_from(["cordial", "--memory-limit", "16G", "open"]).into_diagnostic()?;
    assert_eq!(resolve(before.memory_limit, Some("12G")), 16 * GIB);
    let after =
        Cli::try_parse_from(["cordial", "open", "--memory-limit", "10%"]).into_diagnostic()?;
    assert_eq!(resolve(after.memory_limit, Some("12G")), 32 * GIB / 10);
    let none_at_all =
        Cli::try_parse_from(["cordial", "open", "--memory-limit", "0"]).into_diagnostic()?;
    assert_eq!(resolve(none_at_all.memory_limit, Some("12G")), 0);

    // A setting that is not a limit is refused with the default, not obeyed or fatal.
    assert_eq!(resolve(None, Some("lots")), 24 * GIB);
    Ok(())
}

#[test]
fn cordial_toml_can_set_the_limit() -> Result<()> {
    let project = tempfile::tempdir().into_diagnostic()?;
    let home = tempfile::tempdir().into_diagnostic()?;
    assert_eq!(
        load_cordial_config(project.path(), home.path()).memory_limit(),
        &None,
        "unset by default"
    );
    std::fs::write(
        project.path().join("cordial.toml"),
        "memory_limit = \"60%\"\n",
    )
    .into_diagnostic()?;
    let config = load_cordial_config(project.path(), home.path());
    assert_eq!(config.memory_limit().as_deref(), Some("60%"));
    assert_eq!(
        resolve_memory_limit_with(None, config.memory_limit().as_deref(), Some(10 * GIB)).bytes(),
        6 * GIB
    );
    Ok(())
}

/// Run `cordial open` on a small workspace with `arguments` and the contents of `cordial.toml`.
fn run_open(arguments: &[&str], config: Option<&str>) -> Result<(Option<i32>, String)> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let project = scratch.path().join("mini");
    let store = scratch.path().join("store");
    support::mini_workspace::write(&project)?;
    if let Some(config) = config {
        std::fs::write(project.join("cordial.toml"), config).into_diagnostic()?;
    }
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cordial"))
        .arg("--project")
        .arg(&project)
        .arg("--store-home")
        .arg(&store)
        .args(arguments)
        .arg("open")
        .output()
        .into_diagnostic()?;
    Ok((
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    ))
}

#[test]
fn a_run_over_its_limit_stops_itself_and_says_where_and_how_to_raise_it() -> Result<()> {
    // Loading a workspace takes far more than this.
    let (code, text) = run_open(&["--memory-limit", "16M"], None)?;
    assert_eq!(
        code,
        Some(MEMORY_LIMIT_EXIT),
        "stopped by its own limit: {text}"
    );
    assert!(text.contains("cordial stopped itself"), "{text}");
    assert!(
        text.contains("--memory-limit"),
        "it says how to raise it: {text}"
    );
    assert!(text.contains("during \""), "it says where it was: {text}");
    Ok(())
}

#[test]
fn a_share_of_what_is_available_bounds_a_run_and_is_named_in_the_message() -> Result<()> {
    // A hundredth of a percent of what is available is a few megabytes.
    let (code, text) = run_open(&["--memory-limit", "0.01%"], None)?;
    assert_eq!(code, Some(MEMORY_LIMIT_EXIT), "{text}");
    assert!(
        text.contains("0.01% of the") && text.contains("available when it started"),
        "the message says what the limit was a share of: {text}"
    );
    Ok(())
}

#[test]
fn the_limit_in_cordial_toml_stops_a_run_too() -> Result<()> {
    let (code, text) = run_open(&[], Some("memory_limit = \"16M\"\n"))?;
    assert_eq!(code, Some(MEMORY_LIMIT_EXIT), "{text}");
    assert!(text.contains("cordial stopped itself"), "{text}");
    Ok(())
}

/// The scaffolding is let go of once a process holds its share of the budget, with no pressure to
/// say so: pressure begins at sixty per cent, and what is built in one step passes it before it is
/// felt. With no budget or no share, or room to spare, it stays.
#[test]
fn the_scaffolding_goes_once_a_process_holds_its_share_of_the_budget() {
    let budget = 1000 * 1024 * 1024;
    assert!(
        !cordial::scaffolding_due(299 * 1024, budget, 30),
        "below the line it stays"
    );
    assert!(
        cordial::scaffolding_due(300 * 1024, budget, 30),
        "at the line it goes"
    );
    assert!(
        cordial::scaffolding_due(900 * 1024, budget, 30),
        "and above it"
    );
    assert!(
        cordial::scaffolding_due(150 * 1024, budget, 15),
        "a lean run has an earlier line"
    );
    assert!(
        !cordial::scaffolding_due(150 * 1024, budget, 50),
        "a roomy run a later one"
    );
    assert!(
        !cordial::scaffolding_due(900 * 1024, 0, 30),
        "no budget is no line"
    );
    assert!(
        !cordial::scaffolding_due(900 * 1024, budget, 0),
        "no share is no line"
    );
    assert!(
        !cordial::scaffolding_due(900 * 1024, 100 * 1024 * 1024 * 1024, 30),
        "with memory to spare it stays"
    );
}

/// Parsimony is a name or a share, the flag beats `cordial.toml`, a setting that is not understood
/// says so and the default stands, and the default is the middle one.
#[test]
fn parsimony_is_a_name_or_a_share_with_a_default_in_the_middle() {
    assert_eq!(cordial::parse_parsimony("lean"), Ok(15));
    assert_eq!(cordial::parse_parsimony("Balanced"), Ok(30));
    assert_eq!(cordial::parse_parsimony("roomy"), Ok(50));
    assert_eq!(cordial::parse_parsimony("off"), Ok(0));
    assert_eq!(cordial::parse_parsimony("40%"), Ok(40));
    assert_eq!(cordial::parse_parsimony("40"), Ok(40));
    assert!(cordial::parse_parsimony("101").is_err());
    assert!(cordial::parse_parsimony("frugal").is_err());
    assert_eq!(
        cordial::resolve_parsimony(None, None),
        cordial::DEFAULT_PARSIMONY_PERCENT
    );
    assert_eq!(cordial::resolve_parsimony(None, Some("lean")), 15);
    assert_eq!(cordial::resolve_parsimony(Some(50), Some("lean")), 50);
    assert_eq!(
        cordial::resolve_parsimony(None, Some("sloppy")),
        cordial::DEFAULT_PARSIMONY_PERCENT,
        "a setting that is not understood leaves the default"
    );
}
