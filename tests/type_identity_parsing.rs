//! Type text: normalisation, parsing, and what is rejected as opaque.
#![cfg(feature = "amenable_std")]

use cordial::testing::{normalize_type_text, parse_type_text};

fn some<T>(value: Option<T>, what: &str) -> miette::Result<T> {
    value.ok_or_else(|| miette::miette!("{what}"))
}

// ---- text ----------------------------------------------------------------

#[test]
fn normalization_removes_stringify_spacing_but_keeps_word_boundaries() {
    cordial::init_tracing();
    assert_eq!(
        normalize_type_text("chrono :: DateTime < chrono :: Utc >"),
        "chrono::DateTime<chrono::Utc>"
    );
    assert_eq!(normalize_type_text("&  mut   Foo"), "&mut Foo");
    assert_eq!(normalize_type_text("dyn   Trait"), "dyn Trait");
}

#[test]
fn parsing_reads_nested_arguments_and_drops_lifetimes() -> miette::Result<()> {
    cordial::init_tracing();
    let parsed = some(parse_type_text("Cow<'a,Map<K,Vec<V>>>"), "did not parse")?;
    assert_eq!(parsed.path(), "Cow");
    assert_eq!(parsed.args().len(), 1);
    let map = &parsed.args()[0];
    assert_eq!(map.path(), "Map");
    assert_eq!(map.args().len(), 2);
    assert_eq!(map.args()[1].path(), "Vec");
    assert_eq!(map.args()[1].args()[0].path(), "V");
    Ok(())
}

#[test]
fn non_path_types_are_opaque_and_unbalanced_text_is_rejected() -> miette::Result<()> {
    cordial::init_tracing();
    for text in ["&str", "(A,B)", "[u8;4]", "dyn Fn(A)->B", "*const T"] {
        let parsed = some(parse_type_text(text), text)?;
        assert_eq!(parsed.path(), text);
        assert!(parsed.args().is_empty());
    }
    assert!(parse_type_text("Foo<Bar").is_none());
    assert!(parse_type_text("").is_none());
    // `->` inside a generic argument is not a closing bracket.
    let nested = some(parse_type_text("Box<fn(A)->B>"), "did not parse")?;
    assert_eq!(nested.args().len(), 1);
    Ok(())
}
