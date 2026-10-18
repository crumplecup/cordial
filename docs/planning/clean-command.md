# `cordial clean`

Status: **Active**

Delete the regenerable artifacts of one project's store, or of every store with
`--all`, and leave curated data alone.

## Problem

Cordial writes a lot into its store, and nothing ever removes any of it. Under
`~/.cordial/{project}/` each run leaves:

- `cache/`: cached IR (`{crate}.ir.json`), rustdoc JSON, build artifacts, and
  `rustdoc-target`, a whole cargo target directory for the rustdoc JSON builds. It
  is the large one, and it grows with every crate and toolchain the project has
  been analyzed with.
- `findings/`: reports, checklists, CSV inventories, and summaries.

Shared across projects there is also `~/.cordial/sysroot/`, the cached std-family
rustdoc. Projects that were analyzed once and never again keep their store forever.
The only way to reclaim the space is to find the directories and delete them by
hand, which is how someone ends up running `rm -rf ~/.cordial` and losing their
exceptions with it.

## Two kinds of store content

The store mixes data that can be rebuilt with data that cannot. `clean` has to tell
them apart. The curated side starts from what `cordial exceptions backup` already
copies, and adds `config/`.

| Kind | Paths under `{store_home}/{project}/` | `clean` |
| --- | --- | --- |
| Regenerable | `cache/`, `findings/`, and `worklist.json` once the [agent worklist](agent-worklist.md) lands | Removes |
| Curated | `exceptions/`, `quality/patches/`, `patches/` (the three subtrees `backup` copies), and `config/` | Never touches |

Under `{store_home}/` itself, `sysroot/cache/` is regenerable and shared, and
`cordial.toml` is curated.

The shared sysroot directory has its own `.cordial-sysroot` marker. Older
sysroot directories are recognized only when they have the expected
`cache/rustdoc/` and `cache/builds/` layout. A separate `sysroot.lock` protects
cache writes and cleaning across projects.

So "wipe the whole cache" means every regenerable path in every project store plus
the shared sysroot cache. It does not mean deleting `~/.cordial`. A user who wants
the curated data gone can delete it by hand, which takes a deliberate act and not a
flag.

## CLI

```sh
cordial clean              # this project's regenerable artifacts
cordial clean --all        # every project's, plus the shared sysroot cache
cordial clean --dry-run    # show what would be removed, remove nothing
```

`--dry-run` combines with `--all`. `-p`, `--store-home`, and `CORDIAL_HOME` pick
the store as they do for every other command; `--crate-name` has no effect, since
a store belongs to a project and not to a crate.

There is no confirmation prompt. Everything `clean` removes can be rebuilt, and a
prompt would hang an agent that has no one to answer it. `--dry-run` is the preview.

```text
$ cordial clean
Removed from ~/.cordial/cordial:
  cache       1.4 GiB
  findings    212 KiB
Freed 1.4 GiB. Kept: exceptions, quality/patches, patches, config.
The next run rebuilds the cache, so it will be slower.
```

```text
$ cordial clean --all --dry-run
Would remove:
  ~/.cordial/cordial     1.4 GiB
  ~/.cordial/homecoming  610 MiB
  ~/.cordial/amenable    2.1 GiB
  ~/.cordial/sysroot     340 MiB
Would free 4.4 GiB across 3 projects and the sysroot cache.
Nothing was removed.
```

A store with nothing to remove is not an error: `clean` says so and exits 0.

If the project has a worklist with checked-off items, `clean` says how many it
discarded, since that progress cannot be rebuilt.

## Safety

The command deletes files, so the rules are strict and each is tested.

- **An allowlist, never a tree.** `clean` removes the fixed paths in the table
  above. It never removes a store directory itself, and it never removes a path it
  does not name. A file someone placed in the store survives.
- **It only cleans stores it recognizes.** A directory is a cordial store if it has
  a `.cordial-store` marker, or, for stores written before the marker existed, if
  it has all three of `cache/`, `findings/`, and `exceptions/`, which is exactly
  what `StoreLayout::ensure_dirs` creates. `ensure_dirs` writes the marker from now
  on. This is what stops `--store-home ~` or a project named `Documents` from
  pointing `clean` at a directory that happens to contain a `cache/` folder.
- **`--all` only walks the store home's immediate children,** and only cleans the
  ones it recognizes. Everything else is listed as skipped.
- **It refuses a store that is, or contains, the project's source root.** A
  `--store-home` that puts the store on top of the repository would otherwise make
  `cache/` mean the project's own folder.
- **It does not follow symlinks.** A link inside a regenerable path is removed as a
  link, and its target is left alone.
- **Failures do not stop the run.** `clean` carries on after a path it cannot
  remove, lists each failure, and exits non-zero at the end.
- **Preview and action are the same plan.** `--dry-run` prints the plan that a real
  run would execute, so what it shows is what gets deleted.

## Design

### Classifying the layout

The store layout owns the classification, next to the paths it describes.
`StoreLayout` gains `regenerable_paths()` and `curated_paths()`, and `SysrootCache`
gains `regenerable_paths()` and its own marker. A test lists the directories `ensure_dirs` creates and
fails if any is in neither set, so a new store directory has to be classified when
it is added. Without that, a new directory would be quietly kept or quietly
deleted by default.

### Plan, then execute

The library exposes two public functions and no output:

- `plan_clean(scope) -> CleanPlan` walks the stores, applies the recognition and
  refusal rules, and returns each path to remove with its size and which store it
  belongs to. It also returns the stores it skipped and why.
- `run_clean(plan) -> CleanReport` removes the paths in a plan and returns what it
  freed and what failed.

`scope` is either one store or all of them. The CLI prints the plan for
`--dry-run` and executes it otherwise. Sizes are computed while planning, before
any removal, so the report's totals do not depend on what deletion managed to do.

### Running alongside other commands

`clean` takes the store lock from the agent worklist plan, exclusively, for each
store it cleans, so it never deletes `cache/` out from under a running gate. For one
project it waits for the lock like every other command, with a progress line naming
the holder. With `--all` it does not wait: a store that is busy is skipped, listed
in the output, and counts as a failure for the exit status, since the clean was
incomplete. The shared sysroot cache uses a separate lock held during sysroot
builds; `clean --all` skips that cache and reports an incomplete clean when it
is busy.

If `clean` ships before that lock exists, it ships without it, and the command's
help says not to run it during a gate.

### Progress

Deleting several gigabytes of build output takes seconds. `clean` reports progress
through the same `ProgressSink` as the other long commands, including the
line-oriented mode for non-terminal stderr, so an agent can tell it is working.

### CLI wiring

Following [One crate, CLI in the library](one-crate-cli-layout.md): `Commands`
gains `Clean { all, dry_run }`, and its `act` calls a function in `src/cli/` that
takes plain values, not clap types. New failures are native sources on
`CordialErrorKind`: a store that was refused (with the reason) and a store that was
busy. I/O failures use the variants that already exist.

## Phases

1. **Classify the layout.** `regenerable_paths()` and `curated_paths()` on
   `StoreLayout` and `SysrootCache`, the `.cordial-store` marker, and the test that
   every directory `ensure_dirs` creates is classified.
2. **Plan and execute.** `plan_clean`, `run_clean`, the recognition and refusal
   rules, and sizes.
3. **The command.** `cordial clean`, `--all`, `--dry-run`, the output, and
   progress.
4. **The lock.** Take and honor the store lock, once the agent worklist plan has
   introduced it.
5. **Docs.** The Store section of `docs/running-cordial.md` says what `clean`
   removes and keeps, and the README command list gains it.

## Verification

Tests live in `tests/`, against temporary stores.

- Curated paths survive: `exceptions/`, `quality/patches/`, `patches/`, `config/`,
  and the store home's `cordial.toml` are all present after `clean` and after
  `clean --all`.
- Regenerable paths are removed, and a stray file placed in the store survives.
- `--dry-run` removes nothing, and its plan equals the paths a real run removes.
- A directory without the marker and without the three legacy directories is not
  cleaned, alone or under `--all`.
- A store that equals or contains the project root is refused.
- A symlink inside `cache/` is removed and its target is not.
- A store with nothing to remove exits 0, and a failed removal exits non-zero after
  the other paths have been handled.
- With the lock in place: `clean` for one project waits for a held lock, and
  `clean --all` skips a busy store and exits non-zero.
- The classification test described above.
- `just check-all`, `just check-features`, and `just cordial-gate` before
  promoting `dev`.

## Risks

- **Store names are not unique.** `project_slug_from_path` uses only the final
  directory name, so two checkouts both named `api` share one store. `clean` for
  either removes the cache for both. That costs a rebuild and nothing else, since
  only regenerable paths go, but the sharing is older than this command and `clean`
  does not fix it.
- **The next run is slow.** After a clean, the first run rebuilds rustdoc JSON and
  IR from nothing, and `--all` also drops the sysroot cache, which is rebuilt with
  a nightly toolchain. The output says so, and `--dry-run` shows the cost in bytes
  first.
- **The marker is new.** Stores that predate it are recognized by their three
  directories, so no migration is needed, and the marker appears the next time the
  store is used.
- **Curated means "never", including for the user.** There is no flag to remove
  exceptions. If that turns out to be wanted, it should be its own explicit option
  with a prompt-free but unmissable name, and not a side effect of `--all`.

## Decisions to settle

- **Reports with the cache (decided).** `clean` removes `findings/` along with
  `cache/`. Reports are rebuilt by the next run.
- **Shared sysroot safety (decided).** Recognize the sysroot by its marker or
  legacy layout, and hold a separate lock during sysroot builds and cleaning.
- **Finer scopes.** A way to remove only the build output (`rustdoc-target`), the
  largest and slowest-to-rebuild part, would let a user reclaim space without
  losing the cheaper caches. It is left out until someone asks.

## Status

- [x] Phase 1: classify the layout, marker
- [x] Phase 2: `plan_clean`, `run_clean`
- [x] Phase 3: `cordial clean`, `--all`, `--dry-run`
- [x] Phase 4: store lock
- [ ] Phase 5: docs
