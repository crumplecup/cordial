//! Render existing syntax tokens without parsing their text again.
use ra_ap_syntax::{SyntaxNode, SyntaxToken};

#[tracing::instrument(level = "trace", skip(node))]
pub(crate) fn render(node: &SyntaxNode) -> String {
    let tokens: Vec<_> = node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    sequence(&tokens, &mut 0, None)
}

#[tracing::instrument(level = "trace", skip(tokens, index))]
fn sequence(tokens: &[SyntaxToken], index: &mut usize, close: Option<&str>) -> String {
    let mut parts: Vec<String> = Vec::new();
    while let Some(token) = tokens.get(*index) {
        *index += 1;
        let text = token.text();
        if close == Some(text) {
            break;
        }
        let closing = match text {
            "(" => Some(")"),
            "[" => Some("]"),
            "{" => Some("}"),
            _ => None,
        };
        if let Some(closing) = closing {
            let body = sequence(tokens, index, Some(closing));
            parts.push(if text == "{" && !body.is_empty() {
                format!("{{ {body} }}")
            } else {
                format!("{text}{body}{closing}")
            });
        } else if text == ":" && parts.last().is_some_and(|part| part == ":") {
            if let Some(part) = parts.last_mut() {
                part.push(':');
            }
        } else {
            parts.push(text.to_string());
        }
    }
    parts.join(" ")
}
