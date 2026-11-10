//! Contract clause token trees read from rust-analyzer's lexer.
//!
//! Verus and Creusot clauses live in macro token trees the Rust grammar does
//! not interpret, so the scanner works on delimiter-balanced tokens. This tree
//! comes from the same lexer and token-tree parser as the rest of the loader,
//! carrying each token's source line and punctuation adjacency.

use ra_ap_syntax::{
    AstNode, Edition, NodeOrToken, SourceFile, SyntaxKind, SyntaxToken,
    ast::{self, HasModuleItem},
};
use tracing::instrument;

/// Offset of the clause text inside the macro call that wraps it for parsing.
const WRAPPER_PREFIX: usize = "m!{".len();

/// Bracket pair around a [`TokenKind::Group`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Delimiter {
    Parenthesis,
    Brace,
    Bracket,
}

/// One leaf token or delimited group, with the line it starts on.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters, derive_new::new)]
#[new(visibility = "pub(super)")]
pub(super) struct Token {
    #[getter(copy)]
    line: u32,
    kind: TokenKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TokenKind {
    /// Identifiers, keywords and lifetimes.
    Ident(String),
    /// One punctuation character; `joint` when another follows immediately.
    Punct { ch: char, joint: bool },
    Literal(String),
    Group(Delimiter, Vec<Token>),
}

impl Token {
    /// The identifier text, if this is an identifier or keyword.
    #[instrument(level = "debug", skip(self))]
    pub(super) fn ident(&self) -> Option<&str> {
        match &self.kind {
            TokenKind::Ident(text) => Some(text),
            _ => None,
        }
    }

    /// Whether this is the punctuation character `wanted`.
    #[instrument(level = "trace", skip(self))]
    pub(super) fn is_punct(&self, wanted: char) -> bool {
        matches!(&self.kind, TokenKind::Punct { ch, .. } if *ch == wanted)
    }

    /// The contents of a group with this delimiter.
    #[instrument(level = "debug", skip(self, wanted))]
    pub(super) fn group(&self, wanted: Delimiter) -> Option<&[Token]> {
        match &self.kind {
            TokenKind::Group(delimiter, inner) if *delimiter == wanted => Some(inner),
            _ => None,
        }
    }
}

/// Lex and bracket-match `text`; `None` when its delimiters do not balance or
/// a token is malformed. Lines count from 1 at the start of `text`.
#[instrument(level = "debug", skip(text))]
pub(super) fn parse(text: &str) -> Option<Vec<Token>> {
    let parse = SourceFile::parse(&format!("m!{{{text}}}"), Edition::CURRENT);
    if !parse.errors().is_empty() {
        return None;
    }
    let tree = parse.tree().items().find_map(|item| match item {
        ast::Item::MacroCall(call) => call.token_tree(),
        _ => None,
    })?;
    Some(children(&tree, &LineStarts::new(text)))
}

#[instrument(level = "trace", skip_all)]
fn children(tree: &ast::TokenTree, lines: &LineStarts) -> Vec<Token> {
    let mut out = Vec::new();
    let mut elements = tree.syntax().children_with_tokens().peekable();
    // The first and last elements are the group's own delimiters.
    elements.next();
    while let Some(element) = elements.next() {
        match element {
            NodeOrToken::Node(node) => {
                let Some(inner) = ast::TokenTree::cast(node) else {
                    continue;
                };
                let Some(open) = inner.syntax().first_token() else {
                    continue;
                };
                let delimiter = match open.kind() {
                    SyntaxKind::L_PAREN => Delimiter::Parenthesis,
                    SyntaxKind::L_CURLY => Delimiter::Brace,
                    _ => Delimiter::Bracket,
                };
                out.push(Token::new(
                    lines.line_of(&open),
                    TokenKind::Group(delimiter, children(&inner, lines)),
                ));
            }
            NodeOrToken::Token(token) => {
                if elements.peek().is_none() || token.kind().is_trivia() {
                    continue;
                }
                out.push(Token::new(lines.line_of(&token), leaf(&token)));
            }
        }
    }
    out
}

#[instrument(level = "debug", skip(token))]
fn leaf(token: &SyntaxToken) -> TokenKind {
    let kind = token.kind();
    if kind.is_punct() {
        let joint = token
            .next_token()
            .is_some_and(|next| next.kind().is_punct() && !next.kind().is_delimiter());
        TokenKind::Punct {
            ch: token.text().chars().next().unwrap_or_default(),
            joint,
        }
    } else if kind.is_literal() {
        TokenKind::Literal(token.text().to_string())
    } else {
        TokenKind::Ident(token.text().to_string())
    }
}

/// Byte offsets of line starts in the unwrapped clause text.
struct LineStarts(Vec<usize>);

impl LineStarts {
    #[instrument(level = "debug")]
    fn new(text: &str) -> Self {
        Self(
            std::iter::once(0)
                .chain(text.match_indices('\n').map(|(at, _)| at + 1))
                .collect(),
        )
    }

    #[instrument(level = "trace", skip(self, token))]
    fn line_of(&self, token: &SyntaxToken) -> u32 {
        let offset = usize::from(token.text_range().start()).saturating_sub(WRAPPER_PREFIX);
        self.0.partition_point(|start| *start <= offset) as u32
    }
}

/// Tokens separated by single spaces, except between adjacent punctuation.
#[instrument(level = "trace", skip_all)]
pub(super) fn render(tokens: &[Token]) -> String {
    let mut out = String::new();
    let mut glue = true;
    for token in tokens {
        if !glue {
            out.push(' ');
        }
        glue = false;
        match &token.kind {
            TokenKind::Ident(text) | TokenKind::Literal(text) => out.push_str(text),
            TokenKind::Punct { ch, joint } => {
                out.push(*ch);
                glue = *joint;
            }
            TokenKind::Group(Delimiter::Parenthesis, inner) => {
                out.push_str(&format!("({})", render(inner)));
            }
            TokenKind::Group(Delimiter::Bracket, inner) => {
                out.push_str(&format!("[{}]", render(inner)));
            }
            TokenKind::Group(Delimiter::Brace, inner) if inner.is_empty() => out.push_str("{ }"),
            TokenKind::Group(Delimiter::Brace, inner) => {
                out.push_str(&format!("{{ {} }}", render(inner)));
            }
        }
    }
    out
}

/// Split a flat token sequence on its top-level commas.
#[instrument(level = "trace", skip_all)]
pub(super) fn split_top_level_commas(tokens: &[Token]) -> Vec<Vec<Token>> {
    let mut segments = Vec::new();
    let mut current = Vec::new();
    for token in tokens {
        if token.is_punct(',') {
            segments.push(std::mem::take(&mut current));
        } else {
            current.push(token.clone());
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}

/// A clause with every name and literal replaced by `X`, except called names,
/// so clauses with the same call structure group together.
#[instrument(level = "debug")]
pub(in crate::etiquettes::antipatterns) fn shape(snippet: &str) -> String {
    parse(snippet).map_or_else(|| snippet.to_string(), |tokens| render(&shaped(&tokens)))
}

#[instrument(level = "debug", skip(tokens))]
fn shaped(tokens: &[Token]) -> Vec<Token> {
    tokens
        .iter()
        .enumerate()
        .map(|(at, token)| {
            let called = tokens
                .get(at + 1)
                .is_some_and(|next| next.group(Delimiter::Parenthesis).is_some());
            let kind = match &token.kind {
                TokenKind::Ident(_) if called => token.kind.clone(),
                TokenKind::Ident(_) | TokenKind::Literal(_) => TokenKind::Ident("X".into()),
                TokenKind::Group(delimiter, inner) => TokenKind::Group(*delimiter, shaped(inner)),
                TokenKind::Punct { .. } => token.kind.clone(),
            };
            Token::new(token.line, kind)
        })
        .collect()
}
