//! Lower canonical field syntax to the same closed storage contract.
use super::super::native::syntax::{simple_type, type_label};
use super::{Arguments, CopyRequirement, FieldType};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasGenericArgs},
};

impl FieldType {
    #[tracing::instrument(level = "trace", skip(ty))]
    pub(crate) fn collect_native(ty: &ast::Type) -> Self {
        Self {
            label: type_label(ty),
            static_str: static_str(ty),
            borrowed_string: borrowed_string(ty),
            copy: copy(ty),
        }
    }
}
#[tracing::instrument(level = "trace", skip(ty))]
fn static_str(ty: &ast::Type) -> bool {
    match ty {
        ast::Type::RefType(t) => {
            t.mut_token().is_none()
                && t.lifetime().is_some_and(|l| l.syntax().text() == "'static")
                && t.ty().and_then(|t| simple_type(&t)).as_deref() == Some("str")
        }
        ast::Type::ParenType(t) => t.ty().is_some_and(|t| static_str(&t)),
        _ => false,
    }
}
#[tracing::instrument(level = "trace", skip(ty))]
fn borrowed_string(ty: &ast::Type) -> bool {
    match ty {
        ast::Type::RefType(t) => t.ty().is_some_and(|ty| {
            if let ast::Type::PathType(path) = &ty
                && path
                    .path()
                    .and_then(|p| p.segment())
                    .and_then(|s| s.name_ref())
                    .is_some_and(|name| name.text() == "String")
            {
                return true;
            }
            borrowed_string(&ty)
        }),
        ast::Type::ParenType(t) => t.ty().is_some_and(|t| borrowed_string(&t)),
        ast::Type::ArrayType(t) => t.ty().is_some_and(|t| borrowed_string(&t)),
        ast::Type::TupleType(t) => t.fields().any(|t| borrowed_string(&t)),
        ast::Type::PathType(t) => {
            let mut path = t.path();
            while let Some(p) = path {
                if p.segment()
                    .and_then(|s| s.generic_arg_list())
                    .is_some_and(|list| {
                        list.generic_args().any(|arg| {
                            let ast::GenericArg::TypeArg(arg) = arg else {
                                return false;
                            };
                            arg.ty().is_some_and(|ty| borrowed_string(&ty))
                        })
                    })
                {
                    return true;
                }
                path = p.qualifier();
            }
            false
        }
        _ => false,
    }
}
#[tracing::instrument(level = "trace", skip(ty))]
fn copy(ty: &ast::Type) -> CopyRequirement {
    match ty {
        ast::Type::RefType(_) | ast::Type::PtrType(_) | ast::Type::NeverType(_) => {
            CopyRequirement::Always
        }
        ast::Type::ParenType(t) => t.ty().map(|t| copy(&t)).unwrap_or(CopyRequirement::Never),
        ast::Type::ArrayType(t) => t.ty().map(|t| copy(&t)).unwrap_or(CopyRequirement::Never),
        ast::Type::TupleType(t) => CopyRequirement::All(t.fields().map(|t| copy(&t)).collect()),
        ast::Type::PathType(t) => {
            let Some(segment) = t.path().and_then(|p| p.segment()) else {
                return CopyRequirement::Never;
            };
            let Some(name) = segment.name_ref() else {
                return CopyRequirement::Never;
            };
            CopyRequirement::Path {
                name: name.text().to_string(),
                arguments: if let Some(args) = segment.generic_arg_list() {
                    Arguments::Angle(
                        args.generic_args()
                            .map(|a| match a {
                                ast::GenericArg::TypeArg(a) => {
                                    a.ty().map(|t| copy(&t)).unwrap_or(CopyRequirement::Never)
                                }
                                ast::GenericArg::LifetimeArg(_) | ast::GenericArg::ConstArg(_) => {
                                    CopyRequirement::Always
                                }
                                _ => CopyRequirement::Never,
                            })
                            .collect(),
                    )
                } else if segment.parenthesized_arg_list().is_some() {
                    Arguments::Unsupported
                } else {
                    Arguments::None
                },
            }
        }
        _ => CopyRequirement::Never,
    }
}
