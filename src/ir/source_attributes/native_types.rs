//! Attribute-owner keys matching the tracing inventory's method identities.

use ra_ap_syntax::ast::{self, HasGenericArgs};

use tracing::instrument;
#[instrument(level = "debug", skip(ty))]
pub(crate) fn type_key(ty: &ast::Type) -> String {
    match ty {
        ast::Type::PathType(ty) => {
            let Some(segment) = ty.path().and_then(|path| path.segment()) else {
                return "?".into();
            };
            let Some(name) = segment.name_ref() else {
                return "?".into();
            };
            let base = name.text();
            let args: Vec<_> = segment
                .generic_arg_list()
                .into_iter()
                .flat_map(|list| list.generic_args())
                .filter_map(|arg| match arg {
                    ast::GenericArg::TypeArg(arg) => arg.ty().map(|ty| type_key(&ty)),
                    ast::GenericArg::ConstArg(arg) => Some(const_key(&arg)),
                    _ => None,
                })
                .collect();
            if args.is_empty() {
                base.to_string()
            } else {
                format!("{base}<{}>", args.join(", "))
            }
        }
        ast::Type::RefType(ty) => format!(
            "&{}{}",
            if ty.mut_token().is_some() { "mut " } else { "" },
            optional_type(ty.ty())
        ),
        ast::Type::PtrType(ty) => format!(
            "*{} {}",
            if ty.mut_token().is_some() {
                "mut"
            } else {
                "const"
            },
            optional_type(ty.ty())
        ),
        ast::Type::TupleType(ty) => format!(
            "({})",
            ty.fields()
                .map(|ty| type_key(&ty))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ast::Type::SliceType(ty) => format!("[{}]", optional_type(ty.ty())),
        ast::Type::ArrayType(ty) => format!(
            "[{}; {}]",
            optional_type(ty.ty()),
            ty.const_arg()
                .map(|arg| const_key(&arg))
                .unwrap_or_else(|| "_".into())
        ),
        ast::Type::ParenType(ty) => optional_type(ty.ty()),
        _ => "?".into(),
    }
}

#[instrument(level = "debug", skip(ty))]
fn optional_type(ty: Option<ast::Type>) -> String {
    ty.map(|ty| type_key(&ty)).unwrap_or_else(|| "?".into())
}

#[instrument(level = "debug", skip(arg))]
fn const_key(arg: &ast::ConstArg) -> String {
    match arg.expr() {
        Some(ast::Expr::Literal(literal)) => match literal.kind() {
            ast::LiteralKind::IntNumber(number) => number
                .value()
                .map(|value| value.to_string())
                .unwrap_or_else(|_| "_".into()),
            ast::LiteralKind::Bool(value) => value.to_string(),
            _ => "_".into(),
        },
        Some(ast::Expr::PathExpr(path)) => path
            .path()
            .and_then(|path| path.segment())
            .and_then(|segment| segment.name_ref())
            .map(|name| name.text().to_string())
            .unwrap_or_else(|| "?".into()),
        _ => "_".into(),
    }
}
