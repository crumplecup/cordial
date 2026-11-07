//! Compatibility collection at the loader boundary, never in apply consumers.
use super::bindings::{
    BindingCounts, caller_aliases, impl_uses_lifetime, static_string_locals, static_string_params,
};
use super::projection::{Field, InitializerFileFacts, Record, StringOrigin, StringProof};
use super::replacements::{StaticNames, inspect_initializer, value_expression};
use super::tuples::{TupleFileOrigins, tuple_file_origins};
use super::{FieldKind, cfg_text, simple_type_name};
use crate::loader::SharedParsedFile;
use std::collections::{BTreeMap, BTreeSet};
use syn::{Expr, Member, visit::Visit};
use tracing::instrument;

#[derive(Default)]
struct Context {
    aliases: BTreeSet<String>,
    bindings: BTreeMap<String, (String, usize)>,
    tuples: TupleFileOrigins,
}

struct Collector<'a> {
    source: &'a str,
    records: Vec<Record>,
    owner: Option<String>,
    impl_lifetime: Option<String>,
    lifetime_context: bool,
    context: Context,
}

impl InitializerFileFacts {
    #[instrument(level = "debug", skip(file))]
    #[hotpath::measure(impl_type = "InitializerFileFacts")]
    pub(crate) fn collect(file: &SharedParsedFile) -> Self {
        let mut collector = Collector {
            source: file.source(),
            records: Vec::new(),
            owner: None,
            impl_lifetime: None,
            lifetime_context: false,
            context: Context::default(),
        };
        collector.visit_file(file.syntax());
        Self::new(collector.records)
    }
}

impl Collector<'_> {
    #[instrument(level = "trace", skip(self, signature, block))]
    fn body(&mut self, signature: &syn::Signature, block: &syn::Block) {
        let mut counts = BindingCounts::default();
        counts.visit_block(block);
        let mut lifetimes = BTreeSet::from(["static".to_string()]);
        for node in signature
            .inputs
            .iter()
            .filter_map(|arg| match arg {
                syn::FnArg::Typed(arg) => Some(&*arg.ty),
                _ => None,
            })
            .chain(block.stmts.iter().filter_map(|stmt| match stmt {
                syn::Stmt::Local(local) => match &local.pat {
                    syn::Pat::Type(pat) => Some(&*pat.ty),
                    _ => None,
                },
                _ => None,
            }))
        {
            if let syn::Type::Reference(reference) = node
                && let Some(lifetime) = &reference.lifetime
            {
                lifetimes.insert(lifetime.ident.to_string());
            }
        }
        let mut bindings = BTreeMap::new();
        for lifetime in lifetimes {
            for name in static_string_params(signature, &counts, Some(&lifetime), true) {
                bindings.entry(name).or_insert((lifetime.clone(), 0));
            }
            for (name, end) in static_string_locals(
                signature,
                block,
                &counts,
                self.source,
                Some(&lifetime),
                true,
            ) {
                bindings.entry(name).or_insert((lifetime.clone(), end));
            }
        }
        // Static evidence takes precedence over a named lifetime.
        for name in static_string_params(signature, &counts, None, false) {
            bindings.insert(name, ("static".to_string(), 0));
        }
        for (name, end) in static_string_locals(signature, block, &counts, self.source, None, false)
        {
            bindings.insert(name, ("static".to_string(), end));
        }
        let context = Context {
            aliases: caller_aliases(block, &counts),
            bindings,
            tuples: tuple_file_origins(self.source, block, &counts),
        };
        let previous = std::mem::replace(&mut self.context, context);
        self.visit_block(block);
        self.context = previous;
    }

    #[instrument(level = "trace", skip(self, field))]
    fn field(&self, field: &syn::FieldValue) -> Option<Field> {
        let Member::Named(name) = &field.member else {
            return None;
        };
        let name = name.to_string();
        let empty = BTreeSet::new();
        let locals: BTreeMap<_, _> = self
            .context
            .bindings
            .iter()
            .map(|(name, (_, end))| (name.clone(), *end))
            .collect();
        let value = value_expression(&field.expr);
        let constant = match value {
            Expr::Path(path) if path.qself.is_none() => {
                let names: Vec<_> = path
                    .path
                    .segments
                    .iter()
                    .map(|part| part.ident.to_string())
                    .collect();
                match names.as_slice() {
                    [scope, name] if scope == "self" => Some(name.clone()),
                    _ => None,
                }
            }
            _ => None,
        };
        let constants = constant.iter().cloned().collect();
        let no_locals = BTreeMap::new();
        let string_names = StaticNames::builder()
            .aliases(&empty)
            .strings(&constants)
            .params(&empty)
            .locals(&locals)
            .build()
            .ok()?;
        let call_site_names = StaticNames::builder()
            .aliases(&self.context.aliases)
            .strings(&empty)
            .params(&empty)
            .locals(&no_locals)
            .build()
            .ok()?;
        let string = inspect_initializer(
            self.source,
            &field.expr,
            &name,
            field.colon_token.is_none(),
            FieldKind::StaticString,
            string_names,
        )
        .map(|fact| {
            let origin = if let Some(constant) = constant {
                StringOrigin::FileConstant(constant)
            } else if let Expr::Path(path) = value {
                path.path
                    .get_ident()
                    .and_then(|name| self.context.bindings.get(&name.to_string()))
                    .filter(|(lifetime, _)| lifetime != "static")
                    .map(|(lifetime, _)| StringOrigin::Lifetime(lifetime.clone()))
                    .unwrap_or(StringOrigin::Static)
            } else {
                StringOrigin::Static
            };
            StringProof::new(fact, origin)
        });
        let direct = inspect_initializer(
            self.source,
            &field.expr,
            &name,
            field.colon_token.is_none(),
            FieldKind::CallSiteFile,
            call_site_names,
        );
        let file = direct.map(|fact| vec![fact]).or_else(|| {
            matches!(&field.expr, Expr::Path(path) if path.qself.is_none() && path.path.is_ident("file"))
                .then(|| self.context.tuples.at(self.source, field)).flatten()
        }).unwrap_or_default();
        Some(Field::new(
            name,
            cfg_text(self.source, &field.attrs),
            file,
            string,
        ))
    }
}

impl<'ast> Visit<'ast> for Collector<'_> {
    #[instrument(level = "trace", skip(self, _item))]
    fn visit_item_mod(&mut self, _item: &'ast syn::ItemMod) {}

    #[instrument(level = "trace", skip(self, item))]
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let owner = simple_type_name(&item.self_ty);
        let lifetime = item
            .generics
            .lifetimes()
            .next()
            .map(|param| param.lifetime.ident.to_string())
            .filter(|lifetime| {
                owner
                    .as_ref()
                    .is_some_and(|name| impl_uses_lifetime(item, name, lifetime))
            });
        let saved = (
            std::mem::replace(&mut self.owner, owner),
            std::mem::replace(&mut self.impl_lifetime, lifetime),
            std::mem::replace(&mut self.lifetime_context, true),
        );
        syn::visit::visit_item_impl(self, item);
        (self.owner, self.impl_lifetime, self.lifetime_context) = saved;
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let saved = std::mem::replace(&mut self.lifetime_context, false);
        self.body(&item.sig, &item.block);
        self.lifetime_context = saved;
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        self.body(&item.sig, &item.block);
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_expr_struct(&mut self, item: &'ast syn::ExprStruct) {
        self.records.push(Record::new(
            item.path
                .segments
                .iter()
                .map(|part| part.ident.to_string())
                .collect(),
            item.path.get_ident().is_some(),
            self.owner.clone(),
            self.impl_lifetime.clone(),
            self.lifetime_context,
            item.rest.is_some(),
            item.fields
                .iter()
                .filter_map(|field| self.field(field))
                .collect(),
        ));
        syn::visit::visit_expr_struct(self, item);
    }
}
