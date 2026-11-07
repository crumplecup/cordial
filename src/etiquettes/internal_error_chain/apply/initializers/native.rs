//! Constructor provenance collected once from the loader's canonical Rust tree.
use super::native_bindings::{Counts, aliases, strings};
use super::native_tuples::{Tuples, cfg_text};
use super::native_values::{self as values, bare, names, range};
use super::projection::{Field, InitializerFileFacts, Record, StringOrigin, StringProof};
use super::replacements::{InitializerExpressionFact, StorageForm, ValueSource};
use crate::loader::DatabaseSource;
use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxNode,
    ast::{self, HasAttrs, HasGenericArgs, HasGenericParams, HasTypeBounds},
};
use std::collections::{BTreeMap, BTreeSet};
use tracing::instrument;

#[derive(Default)]
struct Context {
    aliases: BTreeSet<String>,
    bindings: BTreeMap<String, (String, usize)>,
    tuples: Tuples,
}
struct Collector<'a, 'db> {
    file: &'a DatabaseSource<'db>,
    records: Vec<Record>,
    owner: Option<String>,
    impl_lifetime: Option<String>,
    lifetime_context: bool,
    context: Context,
}

impl InitializerFileFacts {
    #[instrument(level = "debug", skip(file))]
    #[hotpath::measure(impl_type = "InitializerFileFacts")]
    pub(crate) fn collect_native(file: &DatabaseSource<'_>) -> Self {
        let mut collector = Collector {
            file,
            records: Vec::new(),
            owner: None,
            impl_lifetime: None,
            lifetime_context: false,
            context: Context::default(),
        };
        collector.visit(file.tree().syntax());
        Self::new(collector.records)
    }
}

impl Collector<'_, '_> {
    #[instrument(level = "trace", skip(self, node))]
    fn visit(&mut self, node: &SyntaxNode) {
        if matches!(
            node.kind(),
            SyntaxKind::MODULE | SyntaxKind::TOKEN_TREE | SyntaxKind::ATTR
        ) {
            return;
        }
        if let Some(item) = ast::Impl::cast(node.clone()) {
            let owner = item.self_ty().and_then(|ty| match ty {
                ast::Type::PathType(ty) => ty
                    .path()
                    .filter(|path| path.qualifier().is_none() && values::unanchored(path))
                    .and_then(|path| names(&path).into_iter().next()),
                _ => None,
            });
            let saved = (
                std::mem::replace(&mut self.owner, owner),
                std::mem::replace(&mut self.impl_lifetime, impl_lifetime(&item)),
                std::mem::replace(&mut self.lifetime_context, true),
            );
            for child in node.children() {
                self.visit(&child);
            }
            (self.owner, self.impl_lifetime, self.lifetime_context) = saved;
            return;
        }
        if let Some(function) = ast::Fn::cast(node.clone()) {
            let parent = node.parent().and_then(|node| node.parent());
            let associated = parent
                .as_ref()
                .is_some_and(|node| ast::Impl::can_cast(node.kind()));
            let trait_method = parent.is_some_and(|node| ast::Trait::can_cast(node.kind()));
            if !trait_method {
                if let Some(block) = function.body() {
                    let saved_lifetime = self.lifetime_context;
                    if !associated {
                        self.lifetime_context = false;
                    }
                    let counts = Counts::collect(&block);
                    let context = Context {
                        aliases: aliases(&block, &counts),
                        bindings: strings(&function, &block, &counts),
                        tuples: Tuples::collect(self.file, &block, &counts),
                    };
                    let saved = std::mem::replace(&mut self.context, context);
                    self.visit(block.syntax());
                    self.context = saved;
                    self.lifetime_context = saved_lifetime;
                }
                return;
            }
        }
        if let Some(record) = ast::RecordExpr::cast(node.clone())
            && let Some(path) = record.path()
        {
            let fields = record.record_expr_field_list();
            self.records.push(Record::new(
                names(&path),
                bare(&path).is_some(),
                self.owner.clone(),
                self.impl_lifetime.clone(),
                self.lifetime_context,
                fields
                    .as_ref()
                    .is_some_and(|fields| fields.dotdot_token().is_some()),
                fields
                    .into_iter()
                    .flat_map(|fields| fields.fields())
                    .filter_map(|field| self.field(&field))
                    .collect(),
            ));
        }
        for child in node.children() {
            self.visit(&child);
        }
    }

    #[instrument(level = "trace", skip(self, field))]
    fn field(&self, field: &ast::RecordExprField) -> Option<Field> {
        let name_ref = field.name_ref().or_else(|| match field.expr()? {
            ast::Expr::PathExpr(path) => path.path()?.segment()?.name_ref(),
            _ => None,
        })?;
        let name = name_ref.text().to_string();
        if name
            .chars()
            .next()
            .is_some_and(|value| value.is_ascii_digit())
        {
            return None;
        }
        let cfg = cfg_text(self.file, field);
        let shorthand = field.colon_token().is_none();
        let expr = field.expr();
        let string = if let Some(expr) = &expr {
            values::string(expr, &name, shorthand, &self.context.bindings)
        } else {
            let whole = range(field.name_ref()?.syntax());
            self.context
                .bindings
                .get(&name)
                .filter(|(_, end)| *end <= whole.0)
                .map(|(lifetime, _)| {
                    StringProof::new(
                        InitializerExpressionFact::new(
                            whole,
                            whole,
                            StorageForm::Bare,
                            ValueSource::StaticString {
                                local_binding: true,
                            },
                            name.clone(),
                            shorthand,
                        ),
                        if lifetime == "static" {
                            StringOrigin::Static
                        } else {
                            StringOrigin::Lifetime(lifetime.clone())
                        },
                    )
                })
        };
        let direct = expr
            .as_ref()
            .and_then(|expr| values::file(expr, &name, shorthand, &self.context.aliases));
        let file = direct.map(|fact| vec![fact]).unwrap_or_else(|| {
            let tuple = match &expr {
                Some(ast::Expr::PathExpr(path)) => {
                    path.path().and_then(|path| bare(&path)).as_deref() == Some("file")
                }
                None => shorthand && name == "file",
                _ => false,
            };
            if tuple {
                self.context.tuples.at(
                    cfg.as_deref(),
                    expr.as_ref()
                        .map(|expr| range(expr.syntax()).0)
                        .unwrap_or_else(|| range(name_ref.syntax()).0),
                )
            } else {
                Vec::new()
            }
        });
        Some(Field::new(name, cfg, file, string))
    }
}

#[instrument(level = "trace", skip(item))]
fn impl_lifetime(item: &ast::Impl) -> Option<String> {
    if item.where_clause().is_some() {
        return None;
    }
    let params: Vec<_> = item.generic_param_list()?.generic_params().collect();
    let [ast::GenericParam::LifetimeParam(param)] = params.as_slice() else {
        return None;
    };
    if param.attrs().next().is_some()
        || param.doc_comments().next().is_some()
        || param.type_bound_list().is_some()
    {
        return None;
    }
    let lifetime = param.lifetime()?.syntax().text().to_string();
    let ast::Type::PathType(ty) = item.self_ty()? else {
        return None;
    };
    let path = ty.path()?;
    if path.qualifier().is_some() || !values::unanchored(&path) {
        return None;
    }
    let args: Vec<_> = path.segment()?.generic_arg_list()?.generic_args().collect();
    match args.as_slice() {
        [ast::GenericArg::LifetimeArg(arg)]
            if arg.lifetime()?.syntax().text().to_string() == lifetime =>
        {
            Some(lifetime.trim_start_matches('\'').to_string())
        }
        _ => None,
    }
}
