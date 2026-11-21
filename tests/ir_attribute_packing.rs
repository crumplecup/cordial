//! A function's semantic facts, of which the graph holds one or two for every function, are kept as
//! the bytes they encode to until something reads them, and go back to bytes when asked: the same
//! JSON to everything that reads, written and compared as it always was.

use cordial::{ATTR_SEMANTIC_CONTEXT_FUNCTIONS, ATTR_SEMANTIC_FUNCTION, NodeKind, NodeWeight};
use serde_json::json;

fn facts() -> serde_json::Value {
    json!({ "label": "f", "calls": [1, 2, 3], "nested": { "a": "b", "c": [true, null] } })
}

fn weight_with(key: &str, value: serde_json::Value) -> NodeWeight {
    let mut weight = NodeWeight::new(NodeKind::Module);
    weight.set_attr(key, value);
    weight
}

#[test]
fn a_bulk_attribute_is_bytes_until_it_is_read_and_bytes_again_when_dropped() {
    let mut weight = weight_with(ATTR_SEMANTIC_FUNCTION, facts());
    assert_eq!(weight.packed(), (1, 0), "held as bytes");

    assert_eq!(
        weight.attr(ATTR_SEMANTIC_FUNCTION),
        Some(&facts()),
        "reads as the JSON it was"
    );
    assert_eq!(weight.packed(), (1, 1), "decoded by the read");

    assert_eq!(weight.drop_decoded(), 1);
    assert_eq!(weight.packed(), (1, 0), "bytes again");
    assert_eq!(
        weight.attr(ATTR_SEMANTIC_FUNCTION),
        Some(&facts()),
        "and reads the same"
    );
}

#[test]
fn an_attribute_that_is_not_bulk_is_never_packed() {
    let mut weight = weight_with("qualified_path", json!("crate::f"));
    assert_eq!(weight.packed(), (0, 0));
    assert_eq!(weight.drop_decoded(), 0);
    assert_eq!(weight.attr("qualified_path"), Some(&json!("crate::f")));
}

#[test]
fn what_is_written_and_read_back_is_what_it_always_was() {
    let weight = weight_with(ATTR_SEMANTIC_FUNCTION, facts());
    let written = serde_json::to_value(&weight).unwrap_or_default();
    assert_eq!(
        written["attrs"],
        json!([[ATTR_SEMANTIC_FUNCTION, facts()]]),
        "an attribute is written as its key and its JSON"
    );

    assert_eq!(
        weight.packed(),
        (1, 0),
        "writing leaves the live cache cold"
    );

    let read: NodeWeight =
        serde_json::from_value(written).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(read.packed(), (1, 0), "and read back it is bytes again");
    assert_eq!(read, weight, "equal to what was written");
    assert_eq!(
        read.attrs(),
        vec![(ATTR_SEMANTIC_FUNCTION.to_owned(), facts())]
    );
}

#[test]
fn weights_with_the_same_bulk_attribute_are_equal_and_with_another_are_not() {
    let one = weight_with(ATTR_SEMANTIC_FUNCTION, facts());
    let same = weight_with(ATTR_SEMANTIC_FUNCTION, facts());
    let other = weight_with(ATTR_SEMANTIC_FUNCTION, json!({ "label": "g" }));
    assert_eq!(one, same);
    assert_ne!(one, other);
    // A read does not change what a weight is.
    let _ = same.attr(ATTR_SEMANTIC_FUNCTION);
    assert_eq!(one, same);
}

#[test]
fn the_latest_value_of_a_repeated_bulk_key_is_the_one_read() {
    let mut weight = weight_with(ATTR_SEMANTIC_FUNCTION, json!({ "label": "first" }));
    weight.set_attr(ATTR_SEMANTIC_FUNCTION, json!({ "label": "second" }));
    assert_eq!(
        weight.attr(ATTR_SEMANTIC_FUNCTION),
        Some(&json!({ "label": "second" }))
    );
}

#[test]
fn messagepack_persistence_keeps_packed_values_and_does_not_warm_the_source() {
    let weight = weight_with(ATTR_SEMANTIC_FUNCTION, facts());
    let bytes = rmp_serde::to_vec(&weight).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(weight.packed(), (1, 0));
    let read: NodeWeight = rmp_serde::from_slice(&bytes).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(read.packed(), (1, 0));
    assert_eq!(read, weight);
    assert_eq!(read.attr(ATTR_SEMANTIC_FUNCTION), Some(&facts()));
}

#[test]
fn clones_start_cold_and_decoding_or_dropping_one_cache_leaves_the_other_alone() {
    for key in [ATTR_SEMANTIC_FUNCTION, ATTR_SEMANTIC_CONTEXT_FUNCTIONS] {
        let mut source = weight_with(key, facts());
        let mut cold_clone = source.clone();
        assert_eq!(cold_clone.packed(), (1, 0));
        assert_eq!(source.attr(key), Some(&facts()));
        let warm_clone = source.clone();
        assert_eq!(source.packed(), (1, 1));
        assert_eq!(cold_clone.packed(), (1, 0));
        assert_eq!(warm_clone.packed(), (1, 0));

        assert_eq!(cold_clone.attr(key), Some(&facts()));
        assert_eq!(cold_clone.drop_decoded(), 1);
        assert_eq!(source.packed(), (1, 1));
        assert_eq!(source.drop_decoded(), 1);
        assert_eq!(warm_clone.attr(key), Some(&facts()));
        assert_eq!(source.packed(), (1, 0));
        assert_eq!(warm_clone.packed(), (1, 1));
        assert_eq!(cold_clone, source);
        assert_eq!(warm_clone, source);
    }
}

#[test]
fn replacement_append_and_source_drop_preserve_each_clones_values_and_wire() {
    for key in [ATTR_SEMANTIC_FUNCTION, ATTR_SEMANTIC_CONTEXT_FUNCTIONS] {
        let mut source = weight_with(key, json!({"label": "first"}));
        source.set_attr(key, facts());
        assert_eq!(source.attr(key), Some(&facts()));
        let mut replaced = source.clone();
        let mut appended = source.clone();
        let retained = source.clone();
        replaced.replace_attr(key, json!({"label": "replacement"}));
        appended.set_attr(key, json!({"label": "appended"}));
        assert_eq!(source.attr(key), Some(&facts()));
        assert_eq!(replaced.attr(key), Some(&json!({"label": "replacement"})));
        assert_eq!(appended.attr(key), Some(&json!({"label": "appended"})));
        let written = serde_json::to_value(&source).unwrap_or_else(|error| panic!("{error}"));
        drop(source);
        assert_eq!(retained.packed(), (2, 0));
        assert_eq!(
            serde_json::to_value(&retained).unwrap_or_else(|error| panic!("{error}")),
            written
        );
        assert_eq!(retained.packed(), (2, 0));
        assert_eq!(retained.attr(key), Some(&facts()));
        let attrs = retained.attrs();
        assert_eq!(attrs[0], (key.to_owned(), json!({"label": "first"})));
        assert_eq!(attrs[1], (key.to_owned(), facts()));
        let replaced_wire =
            serde_json::to_value(&replaced).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(replaced_wire["attrs"].as_array().map(Vec::len), Some(2));
        assert_eq!(replaced_wire["attrs"][0][1], json!({"label": "first"}));
    }
}
