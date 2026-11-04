# Uniform lint configuration

## Purpose

Every built-in and plugin etiquette must support the same user controls:

- Enable or disable the whole etiquette.
- Enable or disable an individual rule.
- Override either setting for a folder and its descendants.
- Override either setting for an individual file.

Separating code into a module file gives the user a precise policy boundary
without source annotations or disabling unrelated checks.

## Contract

`Etiquette` and `Rule` require `TomlConfigured`. Its required configuration
key identifies the owning etiquette; rule ids identify individual checks.
The session resolves policy centrally. Implementations supply identity, never
their own interpretation of path precedence or toggle semantics. A custom
plugin receives the same configuration support without adding a config match
arm to Cordial.

Policy applies to assessed findings before reporting, gate accounting, worklist
generation, and apply candidate selection. Disabling a finding does not create
an approved exception. Shared loading and enrichment remain available to other
enabled rules. Path decisions consume existing IR and finding locations, with
no source scan or secondary parse.

## Resolution

Defaults enable all registered checks. Existing etiquette `enabled` switches
remain compatible. Resolve the etiquette default, then its individual rule
switch, then matching folder/file overrides from broadest to narrowest. At
one scope the individual rule switch takes precedence over the etiquette
switch. Equal scopes use declaration order, with the later entry winning.

Paths match complete components, never arbitrary string prefixes. Crate-relative
paths support the original skip-path use case; workspace-relative paths can
select workspace folders. A path override cannot suppress a finding whose
location is unknown. Global rule and etiquette controls still apply to such
findings. An explicit narrow enable can restore a check disabled more broadly;
scheduling must retain etiquettes that can be enabled in any scope.

## Delivery checklist

- [x] Required `TomlConfigured` contract for built-ins and custom plugins.
- [x] Shared TOML policy model, deterministic inheritance, and layered loading.
- [x] Central filtering for crate and workspace assessment.
- [x] Apply honors global, rule, folder, and file policy, including old checklists.
- [x] Preserve existing antipattern skip-path behavior.
- [x] Regression tests for precedence, path boundaries, plugin control, reports,
      and apply selection.
- [x] Document concrete TOML examples and the plugin contract.
- [x] Focused checks, zero-open build-tier submit, scoped commit.

## Validation

Focused tests cover config layering, custom plugins, workspace findings,
report filtering, tier selection, and tracing, crate-attribute, panic, and
cross-file error apply. A compile-fail doctest verifies that a rule cannot
omit `TomlConfigured`. The build-tier submit cleared all 16 implementation
findings and finished with zero open items. Deep-tier checks remain deferred.
