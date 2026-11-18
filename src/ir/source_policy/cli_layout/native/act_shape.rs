//! The body of an `act` as plain data, so that the dispatch a CLI's types make is resolved without its
//! syntax.
//!
//! Resolving which types an `act` calls `act` on reads the body's syntax with the crate's catalog of
//! types (their fields and variants), which is known only when every file has been read. Keeping the
//! syntax until then keeps every tree that has an `act` alive for the whole scan, and a result that
//! holds syntax cannot be saved. These types keep exactly what the resolution reads: the paths,
//! field and method-call chains it follows to the type of an expression, the patterns it binds, and
//! the shape of the `match`, `if let`, `while let` and `let` that scope the bindings. The syntax is
//! turned into them once, in the file's own pass.

use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasArgList, HasLoopBody, HasName},
};
use serde::{Deserialize, Serialize};

use super::syntax;

/// What the resolution reads of an expression whose type it asks for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ExprShape {
    /// A path, by its segments.
    Path(Vec<String>),
    /// A field of an expression.
    Field {
        base: Option<Box<ExprShape>>,
        name: Option<String>,
    },
    /// An expression that has the type of what it wraps: a reference, parentheses, `?`, a prefix
    /// operator, or a call of one of the methods that give back the receiver's type.
    Through(Option<Box<ExprShape>>),
    /// Anything else: no type.
    Other,
}

impl ExprShape {
    #[tracing::instrument(level = "trace", skip(expr))]
    fn of(expr: &ast::Expr) -> Self {
        let inner = |expr: Option<ast::Expr>| expr.map(|expr| Box::new(Self::of(&expr)));
        match expr {
            ast::Expr::PathExpr(p) => {
                Self::Path(p.path().map(|p| syntax::path_names(&p)).unwrap_or_default())
            }
            ast::Expr::FieldExpr(f) => Self::Field {
                base: inner(f.expr()),
                name: f.name_ref().map(|name| name.text().to_string()),
            },
            ast::Expr::MethodCallExpr(c)
                if c.name_ref().is_some_and(|n| {
                    matches!(
                        n.text(),
                        "as_ref" | "clone" | "into" | "unwrap" | "expect" | "as_mut" | "copied"
                    )
                }) =>
            {
                Self::Through(inner(c.receiver()))
            }
            ast::Expr::RefExpr(e) => Self::Through(inner(e.expr())),
            ast::Expr::ParenExpr(e) => Self::Through(inner(e.expr())),
            ast::Expr::TryExpr(e) => Self::Through(inner(e.expr())),
            ast::Expr::PrefixExpr(e) => Self::Through(inner(e.expr())),
            _ => Self::Other,
        }
    }
}

/// What the resolution reads of a pattern it binds names from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum PatShape {
    Ident {
        name: Option<String>,
        sub: Option<Box<PatShape>>,
    },
    Record {
        path: Option<Vec<String>>,
        /// Each field's name (its own, or the name of the binding that stands for it) and pattern.
        fields: Vec<(Option<String>, PatShape)>,
    },
    TupleStruct {
        path: Option<Vec<String>>,
        fields: Vec<PatShape>,
    },
    Tuple(Vec<PatShape>),
    Ref(Option<Box<PatShape>>),
    Or(Vec<PatShape>),
    Paren(Option<Box<PatShape>>),
    Other,
}

impl PatShape {
    #[tracing::instrument(level = "trace", skip(pat))]
    fn of(pat: &ast::Pat) -> Self {
        let inner = |pat: Option<ast::Pat>| pat.map(|pat| Box::new(Self::of(&pat)));
        match pat {
            ast::Pat::IdentPat(p) => Self::Ident {
                name: p.name().map(|name| name.text().to_string()),
                sub: inner(p.pat()),
            },
            ast::Pat::RecordPat(p) => Self::Record {
                path: p.path().map(|path| syntax::path_names(&path)),
                fields: p
                    .record_pat_field_list()
                    .into_iter()
                    .flat_map(|list| list.fields().collect::<Vec<_>>())
                    .filter_map(|field| {
                        let pat = field.pat()?;
                        let name = field.name_ref().map(|n| n.text().to_string()).or_else(|| {
                            ast::IdentPat::cast(pat.syntax().clone())
                                .and_then(|p| p.name())
                                .map(|n| n.text().to_string())
                        });
                        Some((name, Self::of(&pat)))
                    })
                    .collect(),
            },
            ast::Pat::TupleStructPat(p) => Self::TupleStruct {
                path: p.path().map(|path| syntax::path_names(&path)),
                fields: p.fields().map(|pat| Self::of(&pat)).collect(),
            },
            ast::Pat::TuplePat(p) => Self::Tuple(p.fields().map(|pat| Self::of(&pat)).collect()),
            ast::Pat::RefPat(p) => Self::Ref(inner(p.pat())),
            ast::Pat::OrPat(p) => Self::Or(p.pats().map(|pat| Self::of(&pat)).collect()),
            ast::Pat::ParenPat(p) => Self::Paren(inner(p.pat())),
            _ => Self::Other,
        }
    }
}

/// What a node does to the resolution beside having children.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ActMark {
    /// A call of a method called `act`: its receiver's type is called.
    Method(ExprShape),
    /// A call of a function called `act` through a path: the segment before it names the type
    /// called (`Self` meaning the owner), and so does the type of the first argument.
    Call {
        owner: Option<String>,
        first_argument: Option<ExprShape>,
    },
}

/// The expression a `match`, `if let` or `while let` is on: its syntax, which is read for the calls in
/// it, and its shape, which gives the type its patterns bind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Scrutinee {
    pub(crate) node: Box<ActNode>,
    pub(crate) shape: ExprShape,
}

/// What a `let` with an initializer binds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LetInit {
    pub(crate) node: Box<ActNode>,
    pub(crate) shape: ExprShape,
    pub(crate) pattern: Option<PatShape>,
}

/// A node of an `act` body, to the extent the resolution reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ActNode {
    /// A node read through its children, in order, and a mark where it is itself a call of `act`.
    Nodes {
        mark: Option<ActMark>,
        children: Vec<ActNode>,
    },
    Match {
        scrutinee: Option<Scrutinee>,
        arms: Vec<(PatShape, ActNode)>,
    },
    IfLet {
        pattern: PatShape,
        scrutinee: Scrutinee,
        then: Option<Box<ActNode>>,
        otherwise: Option<Box<ActNode>>,
    },
    WhileLet {
        pattern: PatShape,
        scrutinee: Scrutinee,
        body: Option<Box<ActNode>>,
    },
    Let {
        init: Option<LetInit>,
    },
}

impl ActNode {
    pub(super) fn empty() -> Self {
        Self::Nodes {
            mark: None,
            children: Vec::new(),
        }
    }

    fn boxed(node: &SyntaxNode) -> Box<Self> {
        Box::new(Self::of(node).unwrap_or_else(Self::empty))
    }

    fn scrutinee(expr: &ast::Expr) -> Scrutinee {
        Scrutinee {
            node: Self::boxed(expr.syntax()),
            shape: ExprShape::of(expr),
        }
    }

    /// The node as the resolution reads it, or none where it does not read it (a token tree).
    #[tracing::instrument(level = "trace", skip(node))]
    pub(crate) fn of(node: &SyntaxNode) -> Option<Self> {
        if ast::TokenTree::can_cast(node.kind()) {
            return None;
        }
        let mut mark = None;
        if let Some(expr) = ast::Expr::cast(node.clone()) {
            match expr {
                ast::Expr::MethodCallExpr(call) => {
                    if call.name_ref().is_some_and(|n| n.text() == "act")
                        && let Some(receiver) = call.receiver()
                    {
                        mark = Some(ActMark::Method(ExprShape::of(&receiver)));
                    }
                }
                ast::Expr::CallExpr(call) => {
                    if let Some(ast::Expr::PathExpr(path)) = call.expr()
                        && let Some(path) = path.path()
                    {
                        let names = syntax::path_names(&path);
                        if names.last().is_some_and(|n| n == "act") {
                            mark = Some(ActMark::Call {
                                owner: (names.len() >= 2).then(|| names[names.len() - 2].clone()),
                                first_argument: call
                                    .arg_list()
                                    .and_then(|a| a.args().next())
                                    .map(|arg| ExprShape::of(&arg)),
                            });
                        }
                    }
                }
                ast::Expr::MatchExpr(expr) => {
                    let Some(scrutinee) = expr.expr() else {
                        return Some(Self::empty());
                    };
                    let arms = expr
                        .match_arm_list()
                        .into_iter()
                        .flat_map(|list| list.arms().collect::<Vec<_>>())
                        .filter_map(|arm| {
                            let (pat, body) = (arm.pat()?, arm.expr()?);
                            Some((PatShape::of(&pat), *Self::boxed(body.syntax())))
                        })
                        .collect();
                    return Some(Self::Match {
                        scrutinee: Some(Self::scrutinee(&scrutinee)),
                        arms,
                    });
                }
                ast::Expr::IfExpr(expr) => {
                    if let Some(ast::Expr::LetExpr(cond)) = expr.condition()
                        && let (Some(pat), Some(scrutinee)) = (cond.pat(), cond.expr())
                    {
                        return Some(Self::IfLet {
                            pattern: PatShape::of(&pat),
                            scrutinee: Self::scrutinee(&scrutinee),
                            then: expr.then_branch().map(|b| Self::boxed(b.syntax())),
                            otherwise: expr.else_branch().map(|branch| match branch {
                                ast::ElseBranch::Block(b) => Self::boxed(b.syntax()),
                                ast::ElseBranch::IfExpr(b) => Self::boxed(b.syntax()),
                            }),
                        });
                    }
                }
                ast::Expr::WhileExpr(expr) => {
                    if let Some(ast::Expr::LetExpr(cond)) = expr.condition()
                        && let (Some(pat), Some(scrutinee)) = (cond.pat(), cond.expr())
                    {
                        return Some(Self::WhileLet {
                            pattern: PatShape::of(&pat),
                            scrutinee: Self::scrutinee(&scrutinee),
                            body: expr.loop_body().map(|b| Self::boxed(b.syntax())),
                        });
                    }
                }
                _ => {}
            }
        }
        if let Some(local) = ast::LetStmt::cast(node.clone()) {
            return Some(Self::Let {
                init: local.initializer().map(|expr| LetInit {
                    node: Self::boxed(expr.syntax()),
                    shape: ExprShape::of(&expr),
                    pattern: local.pat().map(|pat| PatShape::of(&pat)),
                }),
            });
        }
        Some(Self::Nodes {
            mark,
            children: node
                .children()
                .filter_map(|child| Self::of(&child))
                .collect(),
        })
    }
}
