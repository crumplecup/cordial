//! Which gated code a context switches, and which names that touches.
//!
//! A function's facts can differ between two contexts only if something it
//! depends on is switched by the cfg change. Comparing every gate's composed
//! predicate under the ordinary cfg set and under the context's finds exactly
//! the gates that flip. A function needs its own facts in the context only when
//! it sits inside a flipped gate, holds one, or mentions a name a flipped gate
//! defines, imports or uses.

use super::plan::guard;
use ra_ap_cfg::CfgOptions;
use ra_ap_syntax::{AstNode, SourceFile, SyntaxNode, TextRange, ast, ast::HasName};
use std::collections::BTreeSet;
use tracing::instrument;

/// The gates one file has that a context flips.
#[derive(Debug, Default)]
pub(super) struct Flips {
    hosts: Vec<TextRange>,
    names: BTreeSet<String>,
}

/// Which names of a flipped gate make the code that mentions them relevant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Reach {
    /// Every name written in the gate: the code that mentions any of them.
    Written,
    /// What other code sees of the gate: for a gated function, its own name.
    Seen,
}

impl Flips {
    /// Gates of `tree` whose predicate differs between any `(ordinary, context)`
    /// cfg pair, with the names defined or used inside them.
    #[instrument(level = "debug", skip_all)]
    #[hotpath::measure(impl_type = "Flips")]
    pub(super) fn collect(
        tree: &SourceFile,
        pairs: &[(CfgOptions, CfgOptions)],
        reach: Reach,
    ) -> Self {
        let mut flips = Self::default();
        for attr in tree.syntax().descendants().filter_map(ast::Attr::cast) {
            if !matches!(attr.meta(), Some(ast::Meta::CfgMeta(_))) {
                continue;
            }
            let Some(host) = attr.syntax().parent() else {
                continue;
            };
            let predicate = guard(&host);
            let flipped = pairs.iter().any(|(ordinary, context)| {
                predicate_holds(ordinary, &predicate) != predicate_holds(context, &predicate)
            });
            if !flipped {
                continue;
            }
            flips.hosts.push(host.text_range());
            flips.names.extend(host_names(&host, reach));
        }
        flips
    }

    /// The flips of the gates `hosts`, which are the ones that flip.
    pub(super) fn from_hosts<'a>(
        hosts: impl Iterator<Item = &'a super::gates::CfgHost>,
        reach: Reach,
    ) -> Self {
        let mut flips = Self::default();
        for host in hosts {
            flips.hosts.push(host.range);
            flips.names.extend(match reach {
                Reach::Written => host.written.iter().cloned(),
                Reach::Seen => host.seen.iter().cloned(),
            });
        }
        flips
    }

    /// Whether any name is defined or used inside a flipped gate: when none is, no function
    /// mentions one.
    pub(super) fn has_names(&self) -> bool {
        !self.names.is_empty()
    }

    /// Whether `range` lies inside a flipped gate, or contains one.
    #[instrument(level = "trace", skip(self, range))]
    pub(super) fn touches(&self, range: TextRange) -> bool {
        self.hosts
            .iter()
            .any(|host| host.contains_range(range) || range.contains_range(*host))
    }

    /// Whether `node` mentions a name a flipped gate defines, imports or uses.
    #[instrument(level = "trace", skip_all)]
    pub(super) fn mentions(&self, node: &SyntaxNode) -> bool {
        !self.names.is_empty()
            && node.descendants().any(|child| {
                ast::NameRef::cast(child).is_some_and(|name| self.names.contains(name.text()))
            })
    }
}

/// The names a gate on `host` makes relevant to the code that mentions them.
///
/// A gated function is there or not, and what other code sees of it is its name: what its own body
/// calls or declares changes nothing for them. Any other gated item (an `impl`, a `use`, a type, a
/// module) can change what the names it writes mean, so every name written in it counts.
#[instrument(level = "trace", skip_all)]
pub(super) fn host_names(host: &SyntaxNode, reach: Reach) -> BTreeSet<String> {
    if reach == Reach::Seen
        && let Some(function) = ast::Fn::cast(host.clone())
    {
        return function
            .name()
            .map(|name| name.text().to_string())
            .into_iter()
            .collect();
    }
    host.descendants()
        .filter_map(|node| {
            ast::Name::cast(node.clone())
                .map(|name| name.text().to_string())
                .or_else(|| ast::NameRef::cast(node).map(|name| name.text().to_string()))
        })
        .collect()
}

#[instrument(level = "debug", skip(cfg, predicate))]
#[hotpath::measure]
pub(super) fn predicate_holds(cfg: &CfgOptions, predicate: &ra_ap_cfg::CfgExpr) -> bool {
    cfg.check(predicate).unwrap_or(true)
}
