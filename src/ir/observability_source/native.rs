//! Native projection from the database-issued canonical tree, including inactive code.
use super::{
    ObservabilityBody, ObservabilityFile, ObservabilityFileKind, ObservabilityFunction,
    ObservabilityPrint, SourceObservability, cargo_protocol, has_err_instrument, print_rule,
};
use crate::FileSnapshot;
use crate::loader::{DatabaseSource, module_path_from_src_file};
use ra_ap_syntax::{
    AstNode, WalkEvent,
    ast::{self, HasAttrs, HasName},
};
use std::path::Path;

impl SourceObservability {
    #[tracing::instrument(level = "debug", skip(file))]
    pub(crate) fn native_file(
        file: &DatabaseSource<'_>,
        tree: &Path,
        root: &Path,
    ) -> ObservabilityFile {
        let prefix = module_path_from_src_file(tree, file.path());
        let mut functions = Vec::new();
        let mut prints = Vec::new();
        for node in file.tree().syntax().descendants() {
            if let Some(function) = ast::Fn::cast(node.clone()) {
                if !is_trait_member(&function)
                    && let Some(body) = function.body()
                    && let Some(name) = function.name()
                {
                    let mut facts = body_facts(&body);
                    facts.is_fallible = function
                        .ret_type()
                        .and_then(|ty| ty.ty())
                        .is_some_and(|ty| is_result(&ty));
                    facts.has_err_instrument = function
                        .attrs()
                        .filter_map(|attr| {
                            crate::ir::SourceInstrumentSyntax::from_native_attribute(&attr)
                        })
                        .any(|syntax| has_err_instrument(&syntax));
                    functions.push(ObservabilityFunction {
                        name: name.text().to_string(),
                        modules: modules(function.syntax()),
                        line: file.line(function.syntax().text_range().start()),
                        test: function.attrs().any(|attr| {
                            attr.meta()
                                .and_then(|meta| meta.path())
                                .and_then(|path| path.segment())
                                .and_then(|seg| seg.name_ref())
                                .is_some_and(|name| name.text() == "test")
                        }),
                        body: facts,
                    });
                }
            }
            if let Some(call) = ast::MacroCall::cast(node)
                && let Some(path) = call.path()
                && let Some(rule) = last_name(&path).and_then(|name| print_rule(&name))
            {
                let nested = modules(call.syntax());
                let context = prefix
                    .iter()
                    .chain(&nested)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("::");
                prints.push(ObservabilityPrint {
                    context: if context.is_empty() {
                        "<crate>".into()
                    } else {
                        context
                    },
                    line: file.line(path.syntax().text_range().start()),
                    rule,
                    cargo_protocol: call
                        .token_tree()
                        .is_some_and(|tokens| cargo_protocol(file.text(tokens.syntax()))),
                });
            }
        }
        ObservabilityFile {
            snapshot: FileSnapshot::new(file.path().to_path_buf(), file.source_digest()),
            relative: file
                .path()
                .strip_prefix(root)
                .unwrap_or(file.path())
                .to_path_buf(),
            kind: ObservabilityFileKind::for_path(file.path(), root),
            syntax_valid: true,
            functions,
            prints,
        }
    }
}

#[tracing::instrument(level = "trace", skip(function))]
fn is_trait_member(function: &ast::Fn) -> bool {
    function
        .syntax()
        .parent()
        .and_then(|parent| parent.parent())
        .is_some_and(|parent| ast::Trait::can_cast(parent.kind()))
}

#[tracing::instrument(level = "debug", skip(node))]
fn modules(node: &ra_ap_syntax::SyntaxNode) -> Vec<String> {
    let mut modules: Vec<_> = node
        .ancestors()
        .skip(1)
        .filter_map(ast::Module::cast)
        .filter_map(|module| module.name())
        .map(|name| name.text().to_string())
        .collect();
    modules.reverse();
    modules
}

#[tracing::instrument(level = "debug", skip(path))]
fn last_name(path: &ast::Path) -> Option<String> {
    path.segment()
        .and_then(|seg| seg.name_ref())
        .map(|name| name.text().to_string())
}

#[tracing::instrument(level = "debug", skip(path))]
fn joined_path(path: &ast::Path) -> String {
    let mut names = Vec::new();
    let mut current = Some(path.clone());
    while let Some(path) = current {
        if let Some(name) = last_name(&path) {
            names.push(name);
        }
        current = path.qualifier();
    }
    names.reverse();
    names.join("::")
}

#[tracing::instrument(level = "trace", skip(ty))]
fn is_result(ty: &ast::Type) -> bool {
    match ty {
        ast::Type::PathType(ty) => ty
            .path()
            .and_then(|path| last_name(&path))
            .is_some_and(|name| name.ends_with("Result")),
        ast::Type::RefType(ty) => ty.ty().is_some_and(|ty| is_result(&ty)),
        ast::Type::ParenType(ty) => ty.ty().is_some_and(|ty| is_result(&ty)),
        _ => false,
    }
}

#[tracing::instrument(level = "debug", skip(body))]
fn body_facts(body: &ast::BlockExpr) -> ObservabilityBody {
    let mut facts = ObservabilityBody::default();
    let mut walk = body.syntax().preorder();
    while let Some(event) = walk.next() {
        let WalkEvent::Enter(node) = event else {
            continue;
        };
        if ast::Fn::cast(node.clone()).is_some_and(|function| !is_trait_member(&function)) {
            walk.skip_subtree();
            continue;
        }
        if let Some(call) = ast::CallExpr::cast(node.clone())
            && let Some(ast::Expr::PathExpr(expr)) = call.expr()
            && let Some(path) = expr.path()
            && let Some(name) = last_name(&path)
        {
            facts.note_call(&name, false);
            facts.called_paths.push(joined_path(&path));
        }
        if let Some(call) = ast::MethodCallExpr::cast(node.clone())
            && let Some(name) = call.name_ref()
        {
            facts.note_call(name.text(), true);
        }
        if let Some(path) = ast::Path::cast(node.clone())
            && !node
                .parent()
                .is_some_and(|parent| ast::Path::can_cast(parent.kind()))
            && let Some(name) = last_name(&path)
        {
            facts.note_path(&name);
        }
        if let Some(literal) = ast::Literal::cast(node.clone())
            && let ast::LiteralKind::String(value) = literal.kind()
        {
            facts.has_rust_log_literal |= value.value().is_ok_and(|value| value == "RUST_LOG");
        }
        if let Some(call) = ast::MacroCall::cast(node) {
            facts.has_error_emission |= call
                .path()
                .and_then(|path| last_name(&path))
                .is_some_and(|name| matches!(name.as_str(), "warn" | "error"));
        }
    }
    facts
}
