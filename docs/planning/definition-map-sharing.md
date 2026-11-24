# Sharing definition maps between a crate and its cfg copies

**Goal.** A cfg context's copy of a crate costs in proportion to what the context changes in it, not to the
crate. Everything rust-analyzer computes for an item that no gate of the context reaches is computed once, for the
ordinary crate, and the copy uses it: the item tree of its file, its macro expansions, the item's id and so
everything keyed by it (signature, body, inference, our function evidence), and the copy's definition map does not
collect it again.

This continues `save-and-sharing.md` (workstream D). Done there: copies only of the crates a context changes (D2),
copies shared between contexts by what they differ in (D1), name-aware dependency edges, and macro calls in shared
files made as the original's (`MacroDefId::make_call`). What remains is the definition map, the item tree it reads and
the ids it hands out.

## What the code says (vendored rust-analyzer 0.0.358)

The identity of an item is made of three things, and two of them name the crate.

1. **`ModuleId` is a tracked struct, not an arena index** (`hir_def/src/lib.rs:581`, `ModuleIdLt`): its fields are the
   crate, the block, the parent module and the name. `DefMap.modules` is `FxIndexMap<ModuleId, ModuleData>`
   (`nameres.rs:1089`), `ModuleData` holds `children: FxIndexMap<Name, ModuleId>` and a `parent`. A tracked struct is
   found again by the query that made it, so the original's query and a copy's make different ids whatever the fields:
   **a copy cannot make the original's `ModuleId`; it has to read it from the original's map.** To be confirmed by a
   test before anything is built on it (open question 1).
2. **`ItemLoc { container: ModuleId, id: AstId<N> }`** (`lib.rs:141`) is what `FunctionId`, `StructId` and the rest
   intern (`impl_intern!`, `lib.rs:247`). `AstId` is the file and the item's position in it. So an item's id is the
   same in a copy as in the original exactly when its container `ModuleId` is.
3. **`FileItemTreeKey { file_id, krate }`** (`item_tree.rs:145`): the item tree of a file is made per crate, because
   lowering evaluates `#[cfg]` against the crate's cfg. A copy lowers every file again, shared or not. This is why
   `file_item_tree_query` ran once per module under a context in a growth fixture (a crate with a gate in its root
   and 3 or 30 modules of a struct, an `impl`, a `use` and a function each: 4 to 31 executions, and nothing else
   grew). The fixture fails by design until S0 and is written into the test file with it, not before.
4. **`MacroCallLoc.krate` and `MacroDefId.krate`** named the crate too; `make_call` now canonicalizes them for a call
   in a file the copy takes as it is (a source root below `TOOL_SOURCE_ROOTS`), through a new `Crate.canonical` input
   field the tool sets on every copy.

What a definition map holds (`nameres.rs:174`): the module map; the prelude and `macro_use_prelude`;
`derive_helpers_in_scope` (keyed by `AstId`); `macro_def_to_macro_id`; diagnostics; and `DefMapCrateData`
(`exported_derives`, `fn_proc_macro_mapping`, registered tools, features), which is per crate. The collector is
`collect_defs` (`collector.rs:63`): `seed_with_top_level`, `collect` (the import and macro fixpoint), `finish`.

Measured on amenable (release, cold, 10 crates, `cordial::def_map` events): copies' maps cost 34 s of 229 s, 84% of
it the resolve phase; the largest copy, 434 modules, takes 4.3 s, about what its original took. Expansions (done) were
about half of the repeated cost of a leaf context.

## Principle

Identity is positional and canonical. Not persisted (ids already persist across runs: the state restores slots as
they were) and not a content fingerprint (two functions with the same text in two modules are two items): the
canonical crate, the module's path, the item's `AstId`. A copy uses the original's identity for what the context
does not affect, and its own for what it does. A fingerprint has a use here, as the cutoff of a copy's map (the
existing `def_map_fingerprint`), not as identity.

## Which modules are unaffected

A module is **unaffected** by a context when nothing the context changes can reach what the module is made of. The
tool and rust-analyzer each decide a part.

*The tool, from the files* (it has the cfg pairs and the text), the **directly affected files**: a gate in the file
flips; the file reads the cfg otherwise (`cfg!`, `include!`, `#[path`, a `cfg_attr` that flips); or its words include
a name a flipped gate defines (`Reach::Defined`, already in `contexts/flips.rs`, taken transitively up the crate
graph by `follows_of`). It gives the set to the copy as a `Crate` input field, `affected_files`.

*Rust-analyzer, from the original's finished map*, the **module closure**: a module is affected if its file is; if
its parent is (a `mod` declaration may flip, and a parent's `children` and scope change with it); if it imports from
an affected module (from the origin of each name in its scope: `ItemScope::entries`, `imports`, `ImportOrExternCrate`,
`item_scope.rs:206,232`, and the module of the definition it resolved to; a glob counts); if it inherits textual macro
scope from an affected module or defines `macro_rules!` or `#[macro_export]` (conservative: exports reach the crate
root); or if the crate is a proc-macro crate (`DefMapCrateData` is per crate). The unaffected modules are what is left;
they are closed downward (a module under an unaffected one is unaffected) because an affected child makes its
parent affected.

The soundness argument is the file-local rule's: a use of a flipped name that is not written in the module's text (a
macro makes it up) is not seen. Every construct not understood makes the module affected; an unsupported crate falls
back to the copy it is today.

## Stages

Each stage lands with its tests and does nothing else, and a switch (`do_not_share_definition_maps` on the options,
like `do_not_share_context_copies`) turns the whole of it off for the equivalence tests.

**S0. Item trees of shared files.** `file_item_tree` canonicalizes its crate: for a copy and a file the copy takes as
it is (source root below `TOOL_SOURCE_ROOTS`, the criterion `make_call` uses), key the query by `(file, original)`.
A file the copy takes as it is has no gate that flips, so its lowering is the original's. No module analysis needed.
Tests: the growth fixture's `file_item_tree_query` count under the context is flat as unchanged modules are added;
the graph is the one copying every crate finds.

**S1. The analysis, with nothing using it.** The tool computes `affected_files` per copy and sets the field; a tracked
query in `hir_def` computes the module closure from the original's map and returns the unaffected subtrees. An event
says how many modules are affected and shared per copy. Tests: soundness cases, below, against hand-built crates;
the event's counts on the growth fixture (affected modules constant, shared modules growing).

**S2. Canonical item ids (Stage A).** In the copy's collector the container of an item in an unaffected module is the
original's `ModuleId`, found by walking `children` by name from the original's root (the origin matches). Every
`ItemLoc` interned in `ModCollector::collect` goes through one function, `container_for(module)`, which returns the
copy's own module for an affected one. Interning then returns the original's `FunctionId`, `StructId` and the rest.
The copy's map is still collected and complete, so consumers are untouched. Downstream sharing follows from the ids:
signatures, bodies, inference, impls, our evidence, and a smaller saved state (no new interned ids for shared items).
Scope entries for unaffected child modules hold the original's `ModuleId`, so a path resolved from the copy hops into
the original's map, which is what a cross-crate path does already. Tests: a function in an unaffected module has the
same id in the context and in the ordinary build; the queries that run per function under the context stay flat as
unchanged modules are added; the graph found equals copying every crate.

**S3. Skip collection (Stage B).** A maximal unaffected subtree is not collected: `mod x;` where `x` is shared adds
the scope entry for the original's `ModuleId`, copies the legacy macros the module would have contributed, and does not
recurse. The copy's map holds the changed modules and the spine above them. This is the stage that turns the
collection's 34 s into a few seconds, and the one that needs the consumers of a crate's whole map to see the shared
modules. They are the iterations of `DefMap::modules()` (`nameres.rs:696`): `Crate::modules` (`hir/src/lib.rs:299`),
`hir/src/lib.rs:3817`, `hir/src/symbols.rs:420`, `hir_def/src/lang_item.rs:49,326`, `hir_ty/src/upvars.rs:250`,
`hir_ty/src/builtin_derive.rs:437,454`, the owner and key walks at `nameres.rs:468,708`, `prime_caches.rs:316`,
`symbol_index.rs:193,387`, and wherever impl tables are built from module scopes. Each either visits the original's
shared modules as well (one `DefMap` method that yields them) or is shown not to need to. The tool's own listing of a
context's functions (`locations`, `context_functions`) is one of them. Tests: one per consumer, by name.

**S4. The tool.** Ownership and retirement: memos keyed by canonical ids are the original's, owned by no copy, and
`finish_context` must not retire them (the owner index already allows several owners; verify that a canonical memo
read from a context does not register the copy as its owner). `item_key` and `copy_functions` map a copy's item to the
original's by path; a canonical item needs no mapping. The merge of contexts' facts compares a function's facts across
contexts: a function present in both under one id is one function.

**S5. Persistence.** `Crate.canonical` and `affected_files` persist with the crate inputs; the ingredient layout
digest changes (a state of the last build is not used, as for any layout change). Copies restored from a state find
their canonical crate through the saved id. Test: a saved state round-trips a context with shared modules and a
restored run computes nothing for them.

## Progress

*S0 done (2026-11-23).* `file_item_tree` keys the query by the original crate for a copy and a file the copy takes as it
is (`hir_expand::is_shared_with_original`, the rule `make_call` already used, now one function). The growth fixture is in
the test file and passes (`a_contexts_item_trees_do_not_grow_with_the_modules_its_gate_does_not_touch`: it measured
4 to 31 item trees with 3 to 30 modules before the change). 72 context, saved-state and base tests pass. `gates_in` still
grows with the files (31 for 30 modules): it is read once per file the first time the context pass classifies it and
is memoized after, so it is a first-use cost and not a per-context one. Open question 1 is settled by reading salsa:
a tracked struct is allocated in the identity map of the query that makes it
(`tracked_struct.rs`: `disambiguate`, `take_restored` by identity within the creating query), so a copy's query cannot
make the original's `ModuleId`; S2 reads it from the original's map.

*S1 done (2026-11-23).* The tool computes, for each copy it makes itself, the files of the original it cannot take as
they are (`affected_files_of`: the files it copied; the files it took from the ordinary build that mention a name a
flipped gate defines, in the crate or in what it depends on; and, when the crate has any flipped name, the files with
a glob import, since a glob from a module that has only flipped names brings in nothing under the ordinary cfg and so
leaves no edge in the original's map). It sets them on the copy (`Crate.affected_files`) and
`hir_def::nameres::shared::shared_modules(copy)` closes them over the original's finished map: parents of affected
modules, modules importing from an affected module (the module each imported definition lives in), modules that define
a macro or inherit macro scope from an affected parent, and a proc-macro crate whole. The query is persisted (a
restored run executes none) and says `affected_modules` and `shared_modules` per copy on `cordial::def_map`. Nothing
uses it yet. Tests: `a_copy_takes_the_modules_its_gate_does_not_reach_from_the_original` (the root is the only
affected module whether there are 3 modules beside it or 30) and `a_module_that_a_flipped_gate_can_reach_is_the_copys_own`
(a named import of the flipped name, a glob import, a macro, the parent of an affected module: affected; a module
importing from an unaffected one: shared). A named import of a flipped name is caught by the tool's word test and not
by the closure, because an import that does not resolve under the ordinary cfg leaves no edge.

*S2 done (2026-11-23).* A copy's collector (`DefCollector::owner_of`) interns the items of a module the copy takes as
it is with the original's module as their container, found by walking the names from the original's root
(`SharedWithOriginal`); the copy's own map is still collected in full, with the original's ids in those modules' scopes.
Switched on per copy by `Crate.share_modules`, set by the tool unless `do_not_share_definition_maps`. Because the id
is the original's, everything keyed by it is the original's too. Tests: with 3 and with 30 unchanged modules, 6 and 60
of the copy's functions are the ordinary build's own and 1 is the copy's (the gated one), and with sharing off all
are its own (`a_function_in_a_module_the_gate_does_not_reach_is_the_original_s_in_the_copy`, measured by the new
`SourceDatabase::context_function_sharing`); the graph found is the same with sharing on and off on four fixtures
(`what_a_context_finds_is_the_same_whether_or_not_it_takes_modules_from_the_original`); 88 context, saved-state and base
tests pass. A risk to look for in S3: a canonical item's own visibility (`pub(crate)`) names the original's root,
and a check from a module of the copy compares it against the copy's tree. Resolution of paths uses the copy's scope
entries (collected in the copy's map), so it is not affected; a visibility-filtered lookup might be. Not yet seen in a test.

*S3 done (2026-11-23).* A copy's collector does not collect a child module the original's closure says is unaffected (with
everything under it, which is unaffected too): `take_shared_child` declares the original's `ModuleId` as the child in
the parent's scope and in `children`, and puts clones of the original's `ModuleData` for the whole subtree in the
copy's `modules` keyed by the original's ids (the subtree root's `parent` is the copy's own parent), so every
iteration of a crate's modules (`Crate::modules`, the impl tables, lang items, the tool's function listing) sees them
and none had to change; resolution into them goes to the original's map by the `ModuleId`'s crate, as a path into
another crate does. A module declared with `#[path]` is not taken, and a crate with a file that can move where a
module's file is (`#[path]`, `include!`, a `cfg_attr` that flips) shares no modules (`moves_modules`, so a module
of the same name is not the original's by accident). `ModuleData`, `ItemScope` and `DeriveMacroInvocation` are
`Clone` for it. The map's event says `taken_from_original`. Tests: 3 and 30 modules beside the gate are taken, none
collected (`a_copy_collects_none_of_the_modules_it_takes_from_the_original`); the graph found is the same with
sharing on and off on five fixtures, now including an affected module that calls `pub(crate)` and `pub(super)`
functions and a `pub(crate)` method of a shared module (the visibility risk of S2: not seen); 90 context,
saved-state, base and cfg-etiquette tests pass. Not measured on amenable.

## Tests (each stage; the method is the complexity ratchet)

Growth: a fixture with 3 and 30 unchanged modules, asserting what grows under the context (item tree queries,
interned ids, modules collected per copy from the `cordial::def_map` event). It is added with S0, which makes it pass.

Soundness, each against a hand-built crate and against copying every crate:
- a `use super::Foo` where `Foo` is flipped in the parent;
- a glob import from an affected module;
- an affected `mod` declaration (a gated module that appears and one that disappears);
- `#[macro_use]` and textual `macro_rules!` scope reaching a later module from an affected one;
- `#[macro_export]` in a module the context otherwise leaves alone;
- an `impl` in a shared module for a type an affected module defines, and the reverse, found by method resolution
  from the copy;
- a trait impl in a shared module;
- a derive in a shared file whose input has a gated field (the file is affected);
- a flipped name used only inside a function body of a module that is otherwise unaffected;
- a proc-macro crate and a crate whose files are not known (both fall back).

Equivalence: the graph found with sharing on equals the one with it off, on every fixture above.

## Open questions (verify before building on them)

1. **Does a copy's query make a different `ModuleId` for the same fields?** Tracked-struct identity is per creating
   query in this salsa; a test that reads both ids settles it before S2.
2. **Can the collector read the original's `ModuleId` for a module by path** without a cycle (the copy's collection
   depends on the original's finished map; the original does not depend on the copy)?
3. **Does resolution handle a `ModuleId` of the original appearing in a copy's scope** the way it handles another
   crate's (the hop through `crate_def_map(module.krate)`), and `super::` and visibility checks across it?
4. **Derive helpers and `macro_def_to_macro_id`** are keyed by `AstId` and `MacroDefId`; do shared modules' entries
   need to be present in the copy's tables for resolution from affected modules (S3)?
5. **Block def maps** (modules made for a function body's items) are keyed by `BlockId` from the owner's expression
   ids: shared with a shared function, and affected with an affected one; confirm there is no path that makes one for a
   block of a shared function under the copy's crate.
6. **The prelude and extern prelude** of a copy are its own; they come from the dependency list, which a copy rewrites
   (follows_of). A shared module's resolution through the original's prelude is the original's: confirm that is right
   when the copy's dependency edge follows to a copy of a library that flipped something the shared module names (it
   mentions the name, so it is affected, so this should not arise).

## Order and size

S0 is small and independent (a day). S1 and S2 are the core; S2 depends on question 1 to 3 and carries most of the
downstream win. S3 is the largest and the riskiest because of its consumers, and it is optional after S2 (it removes
the collection cost the copy still pays, and the memory of the copy's map). S4 and S5 land with S2.

## Measurement

By the existing events: `cordial::def_map` (phase timings and module counts per map, copy or original), the contexts'
`why this context's files are changed`, and the query-step metrics in the fixtures. No new measurement is needed to
decide to do this; the events are for judging each stage.
