//! Type membership, field contracts and parent/source relationships.
use super::{Collector, syntax};
use super::{EnumInfo, ErrorField, StructInfo, VariantInfo, fields::FieldType};
use crate::etiquettes::internal_error_chain::type_graph::is_foreign_type_label;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasAttrs, HasGenericArgs, HasGenericParams, HasName},
};

impl Collector<'_, '_> {
    #[tracing::instrument(level = "trace", skip(self, item))]
    pub(super) fn structure(&mut self, item: &ast::Struct) {
        if syntax::cfg_test(item) {
            return;
        }
        let Some(name) = item.name() else { return };
        let ident = name.text().to_string();
        self.membership(item, &ident);
        self.fields(&ident, None, item.field_list(), item.generic_param_list());
        let mut info = StructInfo {
            ident: ident.clone(),
            type_path: self.qualified(&ident),
            file: self.file.path().to_path_buf(),
            line: self.file.line(item.syntax().text_range().start()),
            snippet: format!("struct {ident}"),
            kind_box_of: None,
            kind_borrowed_of: None,
            kind_unboxed_of: None,
            foreign_source: None,
            has_source_field: false,
            has_file: false,
            has_line: false,
            has_location: false,
        };
        match item.field_list() {
            Some(ast::FieldList::RecordFieldList(list)) => {
                for field in list.fields() {
                    let (Some(name), Some(ty)) = (field.name(), field.ty()) else {
                        continue;
                    };
                    let label = syntax::type_label(&ty);
                    let last = label.rsplit("::").next().unwrap_or(&label);
                    info.has_location |= last == "Location";
                    match name.text() {
                        "kind" => {
                            if matches!(ty, ast::Type::RefType(_)) {
                                if last.ends_with("Kind") {
                                    info.kind_borrowed_of = Some(last.into());
                                }
                            } else if let Some(inner) = box_inner(&ty) {
                                info.kind_box_of =
                                    Some(inner.rsplit("::").next().unwrap_or(&inner).into());
                            } else if last.ends_with("Kind") {
                                info.kind_unboxed_of = Some(last.into());
                            }
                        }
                        "source" => {
                            info.has_source_field = true;
                            if is_foreign_type_label(&label) {
                                info.foreign_source = Some(label);
                            }
                        }
                        "file" => info.has_file = true,
                        "line" => info.has_line = true,
                        _ => {}
                    }
                }
            }
            Some(ast::FieldList::TupleFieldList(list)) => {
                let types: Vec<_> = list.fields().filter_map(|f| f.ty()).collect();
                if let [ty] = types.as_slice() {
                    let label = syntax::type_label(ty);
                    if is_foreign_type_label(&label) {
                        info.snippet = format!("struct {ident}({label})");
                        info.foreign_source = Some(label);
                    }
                }
            }
            _ => {}
        }
        self.catalog.structs.insert(ident, info);
    }
    #[tracing::instrument(level = "trace", skip(self, item))]
    pub(super) fn enumeration(&mut self, item: &ast::Enum) {
        if syntax::cfg_test(item) {
            return;
        }
        let Some(name) = item.name() else { return };
        let ident = name.text().to_string();
        self.membership(item, &ident);
        let mut variants = Vec::new();
        for variant in item.variant_list().into_iter().flat_map(|l| l.variants()) {
            let Some(name) = variant.name() else { continue };
            let name = name.text().to_string();
            self.fields(
                &ident,
                Some(&name),
                variant.field_list(),
                item.generic_param_list(),
            );
            variants.push(VariantInfo {
                snippet: format!("{ident}::{name}"),
                name,
                line: self.file.line(variant.syntax().text_range().start()),
                payloads: field_types(variant.field_list())
                    .into_iter()
                    .map(|(_, t)| syntax::type_label(&t))
                    .collect(),
            });
        }
        self.catalog.enums.insert(
            ident.clone(),
            EnumInfo {
                type_path: self.qualified(&ident),
                file: self.file.path().to_path_buf(),
                line: self.file.line(item.syntax().text_range().start()),
                snippet: format!("enum {ident}"),
                ident,
                variants,
            },
        );
    }
    #[tracing::instrument(level = "trace", skip(self, item))]
    fn membership(&mut self, item: &impl HasAttrs, ident: &str) {
        if crate::ir::derives_error(item) {
            self.catalog.error_impls.insert(ident.into());
        }
        if derives_copy(item) {
            self.catalog.copy_impls.insert(ident.into());
        }
    }
    #[tracing::instrument(level = "trace", skip(self, list, generics))]
    fn fields(
        &mut self,
        owner: &str,
        variant: Option<&str>,
        list: Option<ast::FieldList>,
        generics: Option<ast::GenericParamList>,
    ) {
        for (name, original) in field_types(list) {
            let ty = syntax::simple_type(&original)
                .filter(|name| {
                    !generics.as_ref().is_some_and(|g| {
                        g.generic_params()
                            .filter_map(|p| match p {
                                ast::GenericParam::TypeParam(p) => Some(p),
                                _ => None,
                            })
                            .filter_map(|t| t.name())
                            .any(|n| n.text() == name)
                    })
                })
                .and_then(|name| self.aliases.get(&name))
                .unwrap_or(&original);
            self.catalog.fields.push(ErrorField {
                owner: owner.into(),
                name: name.clone(),
                context: [
                    Some(self.qualified(owner)),
                    variant.map(str::to_string),
                    Some(name),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join("::"),
                file: self.file.path().to_path_buf(),
                line: self.file.line(original.syntax().text_range().start()),
                ty: FieldType::collect(ty),
            });
        }
    }
}
#[tracing::instrument(level = "trace", skip(list))]
fn field_types(list: Option<ast::FieldList>) -> Vec<(String, ast::Type)> {
    match list {
        Some(ast::FieldList::RecordFieldList(l)) => l
            .fields()
            .filter_map(|f| Some((f.name()?.text().to_string(), f.ty()?)))
            .collect(),
        Some(ast::FieldList::TupleFieldList(l)) => l
            .fields()
            .enumerate()
            .filter_map(|(i, f)| Some((format!("_{i}"), f.ty()?)))
            .collect(),
        None => Vec::new(),
    }
}
#[tracing::instrument(level = "trace", skip(ty))]
fn box_inner(ty: &ast::Type) -> Option<String> {
    match ty {
        ast::Type::ParenType(t) => box_inner(&t.ty()?),
        ast::Type::PathType(t) => {
            let segment = t.path()?.segment()?;
            if segment.name_ref()?.text() != "Box" {
                return None;
            }
            segment
                .generic_arg_list()?
                .generic_args()
                .find_map(|a| match a {
                    ast::GenericArg::TypeArg(a) => a.ty().map(|t| syntax::type_label(&t)),
                    _ => None,
                })
        }
        _ => None,
    }
}
#[tracing::instrument(level = "trace", skip(item))]
fn derives_copy(item: &impl HasAttrs) -> bool {
    item.attrs()
        .filter(|a| a.simple_name().as_deref() == Some("derive"))
        .any(|a| {
            let Some(ast::Meta::TokenTreeMeta(m)) = a.meta() else {
                return false;
            };
            let Some(tree) = m.token_tree() else {
                return false;
            };
            let text = syntax::compact(tree.syntax());
            text.get(1..text.len().saturating_sub(1))
                .is_some_and(|t| t.split(',').any(|p| p == "Copy"))
        })
}
