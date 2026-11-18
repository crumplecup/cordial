//! Resolve the existing CLI dispatch contract over the bodies of the `act` functions, as data.
use super::super::catalog::{LayoutCatalog, TypeRec, VariantShape};
use super::act_shape::{ActMark, ActNode, ExprShape, PatShape};
use std::collections::{BTreeMap, BTreeSet};
#[tracing::instrument(level = "debug", skip(catalog, body))]
#[hotpath::measure]
pub(super) fn targets(catalog: &LayoutCatalog, owner: &str, body: &ActNode) -> BTreeSet<String> {
    let mut resolver = Resolver {
        catalog,
        owner,
        bindings: BTreeMap::new(),
        called: BTreeSet::new(),
    };
    resolver.walk(body);
    resolver.called
}
struct Resolver<'a> {
    catalog: &'a LayoutCatalog,
    owner: &'a str,
    bindings: BTreeMap<String, Vec<String>>,
    called: BTreeSet<String>,
}
impl Resolver<'_> {
    #[tracing::instrument(level = "trace", skip(self, types))]
    fn primary(&self, types: &[String]) -> Option<String> {
        if types.iter().any(|t| t == self.owner) {
            return Some(self.owner.into());
        }
        types
            .iter()
            .find(|t| self.catalog.types().contains_key(*t))
            .cloned()
    }
    #[tracing::instrument(level = "trace", skip(self, expr))]
    fn expr_types(&self, expr: &ExprShape) -> Vec<String> {
        match expr {
            ExprShape::Path(names) => {
                if names == &["self"] {
                    vec![self.owner.into()]
                } else if names.len() == 1 {
                    self.bindings.get(&names[0]).cloned().unwrap_or_default()
                } else {
                    Vec::new()
                }
            }
            ExprShape::Field { base, name } => base
                .as_ref()
                .map(|e| self.expr_types(e))
                .and_then(|types| self.primary(&types))
                .and_then(|name| self.catalog.types().get(&name))
                .and_then(|rec| {
                    name.as_ref()
                        .and_then(|name| rec.fields().get(name.as_str()))
                })
                .cloned()
                .unwrap_or_default(),
            ExprShape::Through(inner) => inner
                .as_ref()
                .map(|e| self.expr_types(e))
                .unwrap_or_default(),
            ExprShape::Other => Vec::new(),
        }
    }
    #[tracing::instrument(level = "trace", skip(self, names))]
    fn pattern_rec(
        &self,
        names: &[String],
        scrutinee: Option<&str>,
    ) -> Option<(&TypeRec, Option<String>)> {
        let first = names.first()?;
        let last = names.last()?;
        let owner = if first == "Self" {
            Some(self.owner)
        } else if self.catalog.types().contains_key(first) {
            Some(first.as_str())
        } else {
            scrutinee
        };
        let rec = self.catalog.types().get(owner?)?;
        if names.len() == 1 && (last == "Self" || last == rec.ident()) {
            return Some((rec, None));
        }
        if rec.variants().contains_key(last) {
            return Some((rec, Some(last.clone())));
        }
        (last == rec.ident()).then_some((rec, None))
    }
    #[tracing::instrument(level = "trace", skip(self, pat, out))]
    fn pattern(&self, pat: &PatShape, ty: Option<&str>, out: &mut BTreeMap<String, Vec<String>>) {
        match pat {
            PatShape::Ident { name, sub } => {
                if let (Some(name), Some(ty)) = (name, ty) {
                    out.insert(name.clone(), vec![ty.into()]);
                }
                if let Some(sub) = sub {
                    self.pattern(sub, ty, out);
                }
            }
            PatShape::Record { path, fields } => {
                let rec = path.as_ref().and_then(|names| self.pattern_rec(names, ty));
                let known = rec.and_then(|(rec, variant)| match variant {
                    Some(v) => match rec.variants().get(&v) {
                        Some(VariantShape::Named(fields)) => Some(fields),
                        _ => None,
                    },
                    None => (!rec.fields().is_empty()).then_some(rec.fields()),
                });
                for (name, pat) in fields {
                    let types = name
                        .as_ref()
                        .and_then(|name| known.and_then(|f| f.get(name)))
                        .cloned()
                        .unwrap_or_default();
                    self.bind_types(pat, &types, out);
                }
            }
            PatShape::TupleStruct { path, fields } => {
                if path
                    .as_ref()
                    .and_then(|names| names.last().cloned())
                    .is_some_and(|n| matches!(n.as_str(), "Some" | "Ok"))
                {
                    if let Some(pat) = fields.first() {
                        self.pattern(pat, ty, out);
                    }
                    return;
                }
                let rec = path.as_ref().and_then(|names| self.pattern_rec(names, ty));
                let known = rec
                    .and_then(|(rec, v)| v.and_then(|v| rec.variants().get(&v)))
                    .and_then(|v| match v {
                        VariantShape::Unnamed(f) => Some(f),
                        _ => None,
                    });
                for (i, pat) in fields.iter().enumerate() {
                    let types = known.and_then(|f| f.get(i)).cloned().unwrap_or_default();
                    self.bind_types(pat, &types, out);
                }
            }
            PatShape::Tuple(pats) | PatShape::Or(pats) => {
                for pat in pats {
                    self.pattern(pat, ty, out);
                }
            }
            PatShape::Ref(inner) | PatShape::Paren(inner) => {
                if let Some(pat) = inner {
                    self.pattern(pat, ty, out);
                }
            }
            PatShape::Other => {}
        }
    }
    #[tracing::instrument(level = "trace", skip(self, pat, types, out))]
    fn bind_types(
        &self,
        pat: &PatShape,
        types: &[String],
        out: &mut BTreeMap<String, Vec<String>>,
    ) {
        match pat {
            PatShape::Ident { name, sub } => {
                if let Some(name) = name {
                    out.insert(name.clone(), types.to_vec());
                }
                if let Some(sub) = sub {
                    self.bind_types(sub, types, out);
                }
            }
            PatShape::Ref(inner) => {
                if let Some(pat) = inner {
                    self.bind_types(pat, types, out);
                }
            }
            _ => {
                let ty = types.iter().find(|n| self.catalog.types().contains_key(*n));
                self.pattern(pat, ty.map(String::as_str), out);
            }
        }
    }
    #[tracing::instrument(level = "trace", skip(self, pat, scrutinee, body))]
    fn scoped(&mut self, pat: &PatShape, scrutinee: &ExprShape, body: &ActNode) {
        let ty = self.primary(&self.expr_types(scrutinee));
        let mut extra = BTreeMap::new();
        self.pattern(pat, ty.as_deref(), &mut extra);
        let old = self.bindings.clone();
        self.bindings.extend(extra);
        self.walk(body);
        self.bindings = old;
    }
    #[tracing::instrument(level = "trace", skip(self, node))]
    fn walk(&mut self, node: &ActNode) {
        match node {
            ActNode::Nodes { mark, children } => {
                match mark {
                    Some(ActMark::Method(receiver)) => {
                        self.called.extend(self.expr_types(receiver));
                    }
                    Some(ActMark::Call {
                        owner,
                        first_argument,
                    }) => {
                        if let Some(name) = owner {
                            self.called.insert(if name == "Self" {
                                self.owner.into()
                            } else {
                                name.clone()
                            });
                        }
                        if let Some(arg) = first_argument {
                            self.called.extend(self.expr_types(arg));
                        }
                    }
                    None => {}
                }
                for child in children {
                    self.walk(child);
                }
            }
            ActNode::Match { scrutinee, arms } => {
                if let Some(scrutinee) = scrutinee {
                    self.walk(&scrutinee.node);
                    for (pat, body) in arms {
                        self.scoped(pat, &scrutinee.shape, body);
                    }
                }
            }
            ActNode::IfLet {
                pattern,
                scrutinee,
                then,
                otherwise,
            } => {
                self.walk(&scrutinee.node);
                if let Some(body) = then {
                    self.scoped(pattern, &scrutinee.shape, body);
                }
                if let Some(branch) = otherwise {
                    self.walk(branch);
                }
            }
            ActNode::WhileLet {
                pattern,
                scrutinee,
                body,
            } => {
                self.walk(&scrutinee.node);
                if let Some(body) = body {
                    self.scoped(pattern, &scrutinee.shape, body);
                }
            }
            ActNode::Let { init } => {
                if let Some(init) = init {
                    self.walk(&init.node);
                    let ty = self.primary(&self.expr_types(&init.shape));
                    let mut extra = BTreeMap::new();
                    if let Some(pat) = &init.pattern {
                        self.pattern(pat, ty.as_deref(), &mut extra);
                    }
                    self.bindings.extend(extra);
                }
            }
        }
    }
}
