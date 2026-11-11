//! Choose the cfg contexts that make gated code active somewhere.
//!
//! Every `#[cfg(..)]` in a workspace file is read from the syntax the database
//! already parsed. Composed with the cfgs of its enclosing nodes, it becomes a
//! disjunction of clauses. A clause that no examined state satisfies asks for a
//! new context applying it on top of the current one. Examining that context
//! can reach files and nested gates that were out of reach before, so asking
//! repeats until nothing new is wanted.
//!
//! Clauses are never merged here: exclusive alternatives (`cfg(kani)` against
//! `cfg(creusot)`) would otherwise define the same item twice in one crate.

use super::spell;
use crate::SemanticContextKey;
use ra_ap_cfg::{CfgAtom, CfgExpr, CfgOptions};
use ra_ap_hir::{Crate, Semantics, attach_db};
use ra_ap_ide_db::RootDatabase;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasAttrs},
};
use ra_ap_vfs::FileId;
use std::collections::{BTreeMap, BTreeSet};
use tracing::instrument;

/// Most clauses one predicate may expand to before the rest are dropped.
const CLAUSE_LIMIT: usize = 64;

/// One way to make a predicate true: atoms that must be on and atoms that must be off.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Clause {
    on: BTreeSet<CfgAtom>,
    off: BTreeSet<CfgAtom>,
}

impl Clause {
    #[instrument(level = "debug", skip(atom))]
    fn atom(atom: &CfgAtom, negated: bool) -> Self {
        let mut clause = Self::default();
        if negated {
            clause.off.insert(atom.clone());
        } else {
            clause.on.insert(atom.clone());
        }
        clause
    }

    /// Both clauses at once, or `None` when they contradict.
    #[instrument(level = "debug", skip(self, other))]
    fn and(&self, other: &Self) -> Option<Self> {
        let on: BTreeSet<_> = self.on.union(&other.on).cloned().collect();
        let off: BTreeSet<_> = self.off.union(&other.off).cloned().collect();
        on.is_disjoint(&off).then_some(Self { on, off })
    }

    /// Whether `cfg` already makes this clause true.
    #[instrument(level = "trace", skip(self, cfg))]
    fn holds(&self, cfg: &CfgOptions) -> bool {
        self.on.iter().all(|atom| cfg.check_atom(atom))
            && self.off.iter().all(|atom| !cfg.check_atom(atom))
    }
}

/// Clauses already satisfied by an examined state or already asked for, per
/// file. A clause recorded here never asks for another context.
#[derive(Debug, Default)]
pub(in crate::loader::database) struct Covered(BTreeSet<(FileId, Clause)>);

/// `expr` (or its negation) as a disjunction of clauses.
#[instrument(level = "trace", skip(expr))]
fn clauses(expr: &CfgExpr, negated: bool) -> Vec<Clause> {
    let all = |parts: &[CfgExpr]| {
        parts.iter().fold(vec![Clause::default()], |acc, part| {
            let next = clauses(part, negated);
            acc.iter()
                .flat_map(|left| next.iter().filter_map(|right| left.and(right)))
                .take(CLAUSE_LIMIT)
                .collect()
        })
    };
    let any = |parts: &[CfgExpr]| {
        parts
            .iter()
            .flat_map(|part| clauses(part, negated))
            .take(CLAUSE_LIMIT)
            .collect()
    };
    match (expr, negated) {
        (CfgExpr::Invalid, _) => Vec::new(),
        (CfgExpr::Atom(atom), negated) => vec![Clause::atom(atom, negated)],
        (CfgExpr::Not(inner), negated) => clauses(inner, !negated),
        (CfgExpr::All(parts), false) | (CfgExpr::Any(parts), true) => all(parts),
        (CfgExpr::Any(parts), false) | (CfgExpr::All(parts), true) => any(parts),
    }
}

/// The `cfg` predicates guarding `node`: its own attributes and those of every
/// enclosing node in the same file, composed with `all`.
#[instrument(level = "trace", skip_all)]
pub(super) fn guard(node: &ra_ap_syntax::SyntaxNode) -> CfgExpr {
    let predicates: Vec<CfgExpr> = node
        .ancestors()
        .filter_map(ast::AnyHasAttrs::cast)
        .flat_map(|host| host.attrs().collect::<Vec<_>>())
        .filter_map(|attr| match attr.meta()? {
            ast::Meta::CfgMeta(meta) => Some(CfgExpr::parse_from_ast(meta.cfg_predicate()?)),
            _ => None,
        })
        .collect();
    CfgExpr::All(predicates.into_boxed_slice())
}

/// Contexts wanted by gated code that the currently applied state reaches.
///
/// `current` is the context the database is in. A clause inside a file needs a
/// new context only when no examined state in which that file is active already
/// satisfies it, and then it is applied on top of `current`.
#[instrument(level = "debug", skip(db, covered, current))]
#[hotpath::measure]
pub(in crate::loader::database) fn asked(
    db: &RootDatabase,
    current: &SemanticContextKey,
    covered: &mut Covered,
) -> Vec<SemanticContextKey> {
    attach_db(db, || {
        let sema = Semantics::new(db);
        let mut active: BTreeMap<FileId, Vec<&CfgOptions>> = BTreeMap::new();
        for krate in Crate::all(db)
            .into_iter()
            .filter(|krate| krate.origin(db).is_local())
        {
            let files: BTreeSet<_> = krate
                .modules(db)
                .into_iter()
                .map(|module| {
                    module
                        .definition_source_file_id(db)
                        .original_file_respecting_includes(db)
                        .file_id(db)
                })
                .collect();
            for file in files {
                active.entry(file).or_default().push(krate.cfg(db));
            }
        }
        let mut asked: BTreeSet<SemanticContextKey> = BTreeSet::new();
        for (file, crates) in active {
            let tree = sema.parse(sema.attach_first_edition(file));
            let hosts = tree
                .syntax()
                .descendants()
                .filter_map(ast::Attr::cast)
                .filter_map(|attr| attr.syntax().parent());
            for host in hosts {
                for clause in clauses(&guard(&host), false) {
                    if crates.iter().any(|cfg| clause.holds(cfg)) {
                        covered.0.insert((file, clause));
                        continue;
                    }
                    if covered.0.contains(&(file, clause.clone())) {
                        continue;
                    }
                    if let Some(next) = crates.iter().find_map(|cfg| apply(current, cfg, &clause)) {
                        covered.0.insert((file, clause));
                        asked.insert(next);
                    }
                }
            }
        }
        asked.into_iter().collect()
    })
}

/// `context` with `clause` applied on top of the cfg set `cfg` it produced, or
/// `None` when `cfg` already satisfies it or the two contradict.
#[instrument(level = "trace", skip(cfg, context, clause))]
fn apply(
    context: &SemanticContextKey,
    cfg: &CfgOptions,
    clause: &Clause,
) -> Option<SemanticContextKey> {
    let mut enabled: BTreeSet<String> = context.enabled().iter().cloned().collect();
    let mut disabled: BTreeSet<String> = context.disabled().iter().cloned().collect();
    for atom in &clause.on {
        if !cfg.check_atom(atom) {
            if disabled.contains(&spell(atom)) {
                return None;
            }
            enabled.insert(spell(atom));
        }
    }
    for atom in &clause.off {
        if cfg.check_atom(atom) {
            if enabled.contains(&spell(atom)) {
                return None;
            }
            disabled.insert(spell(atom));
        }
    }
    let next = SemanticContextKey::new(enabled, disabled);
    (next != *context).then_some(next)
}
