# Dependency freshness etiquette

## Status

Active. The landed work reads direct dependency declarations from `Cargo.toml`,
joins them to versions present in `Cargo.lock`, asks Cargo for current
freshness with `cargo update --dry-run --verbose`, classifies patch/minor/major
update observations, emits `dependency-freshness-survey.csv`, and opens
findings for enabled drift classes plus enabled manifest-policy indicators.

## Problem

Dependency freshness is not one lint. The useful signal comes from several
facts with different review strategies:

- manifest intent: exact pins, wildcards, upper bounds, tilde ranges,
  workspace-inherited dependencies, workspace-policy bypasses, path/git/registry
  source kinds
- lockfile state: which package versions are actually resolved right now
- registry state: latest compatible patch, minor, and major releases
- policy: which drift class should be denied, reported, or deferred

The pipeline therefore surveys before it judges. Lockfile and manifest facts
are local and deterministic; registry freshness is collected by Cargo and can
be overridden in tests or reproducible runs with an explicit cache.

## Pipeline slice

The first slice adds:

- `DependencyFreshnessLoader`: reads `Cargo.toml`, `Cargo.lock`, and Cargo's
  verbose dry-run update report
- `DependencyFreshnessSurveyEnricher`: writes one plugin IR node per direct
  dependency declaration
- `DependencyFreshnessSiteProbe`: marks dependency rows that have enabled
  registry-drift or manifest-policy rule ids
- `DependencyFreshnessAssessor`: turns those marked rows into open findings
- `DependencyFreshnessSurveyReporter`: writes the flat CSV survey artifact
- `DependencyFreshnessCsvReporter`: writes the finding queue
- `DependencyFreshnessChecklistReporter`: writes review checkboxes for open
  dependency freshness items
- `DependencyFreshnessSummaryReporter`: writes workspace totals by crate
- `DependencyFreshnessObservation`: maps a locked version plus Cargo's newer
  available version to a drift class, survey indicator, and rule id

With no newer versions or manifest-policy indicators, the etiquette emits only
the survey artifact and therefore keeps `cordial quality --deny-open`
unchanged. When Cargo reports newer versions or a manifest uses an enabled
policy shape, the ordinary probe/assessor path emits findings.

## Config

All drift classes default to enabled:

```toml
[dependency_freshness]
enabled = true
patch = true
minor = true
major = true
manifest_exact_pin = true
manifest_upper_bound = true
manifest_wildcard = true
manifest_tilde = true
manifest_workspace_bypass = true
```

Turning off one drift class or manifest-policy shape suppresses findings for
that rule id only. The survey artifact still records the observation so a
project can change policy without losing visibility into the dependency state.

## Survey indicators

Current local indicators:

- `manifest_exact_pin`
- `manifest_upper_bound`
- `manifest_wildcard`
- `manifest_tilde`
- `manifest_workspace_bypass`
- `manifest_workspace_inherited`
- `lockfile_resolved`
- `lockfile_missing`

Registry indicators from Cargo or the cache:

- `patch_available`
- `minor_available`
- `major_available`

The exact-pin, upper-bound, wildcard, tilde, and workspace-bypass indicators
are promoted into manifest-policy findings. Registry indicators are promoted
into patch, minor, and major findings. Those splits let projects choose
distinct gate behavior for each dependency-maintenance concern.

## Strategy direction

Obvious strategies include:

- deny only patch drift for low-risk automated upkeep
- warn on minor drift but require a human update plan
- report major drift as a design-review item rather than an automatic upgrade
- prefer workspace-level dependency declarations when member manifests diverge
- treat exact pins and upper bounds as policy choices, not automatically stale

The important seam is that strategies operate over already-surveyed facts.
Cargo parsing and registry freshness should not be entangled with remediation
policy.

## Rule taxonomy

Registry-backed findings remain split by update kind:

- `DEPENDENCY-FRESHNESS-PATCH`
- `DEPENDENCY-FRESHNESS-MINOR`
- `DEPENDENCY-FRESHNESS-MAJOR`

That split lets teams deny patch drift in CI while leaving minor and major drift
as review queues.

Manifest-policy findings stay separate from registry drift:

- `DEPENDENCY-FRESHNESS-MANIFEST-EXACT-PIN`
- `DEPENDENCY-FRESHNESS-MANIFEST-UPPER-BOUND`
- `DEPENDENCY-FRESHNESS-MANIFEST-WILDCARD`
- `DEPENDENCY-FRESHNESS-MANIFEST-TILDE`
- `DEPENDENCY-FRESHNESS-MANIFEST-WORKSPACE-BYPASS`

That split treats exact pins and upper bounds as explicit policy choices: a
project can deny them, review them, or leave them as surveyed-only evidence
without changing Cargo's freshness collector. Workspace-bypass findings are
local manifest-policy drift: a member declared its own dependency policy even
though the workspace already has a matching `[workspace.dependencies]` entry.

## Registry seam

The current implementation shells out to Cargo instead of adding a new registry
client to cordial. `cargo update --dry-run --verbose` reports both lockfile
updates Cargo can apply and dependencies that remain unchanged because the
manifest requirement prevents moving to the latest available release.

For deterministic tests or reproducible offline runs, an optional cache at
`{store}/cache/dependency-freshness.toml` overrides the Cargo collector:

```toml
[[package]]
name = "serde"
available_version = "1.0.228"
```

That seam keeps source-specific questions out of the policy path:

- where freshness data came from
- when it was last refreshed
- whether prereleases should be considered
- whether an alternate registry is authoritative
- which strategy turns a patch, minor, or major observation into a finding
