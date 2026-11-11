//! Alias expansion and the caps that keep resolution bounded.
#![cfg(feature = "amenable_std")]

use cordial::testing::{CrateIndex, ResolveCaps, TypeResolver, Unresolved};

#[path = "support/type_identity_rustdoc_crate.rs"]
mod type_identity_rustdoc_crate;

use type_identity_rustdoc_crate::{Krate, chrono, key, path_ty, resolver};

// ---- caps ----------------------------------------------------------------

/// `Chain0 = Chain1 = ... = ChainN = Leaf`.
fn alias_chain(length: u32) -> miette::Result<CrateIndex> {
    let mut k = Krate::new("chain");
    k.strukt(500, "Leaf", &[], &["chain", "Leaf"]);
    let mut items = vec![500];
    for step in 0..length {
        let id = 100 + step;
        let target = if step + 1 == length {
            500
        } else {
            100 + step + 1
        };
        let name = format!("Chain{step}");
        k.alias(
            id,
            &name,
            &[],
            path_ty("next", target, &[]),
            &["chain", &name],
        );
        items.push(id);
    }
    k.set_items(0, &items);
    k.build()
}

#[test]
fn alias_chains_are_followed_up_to_the_depth_cap() -> miette::Result<()> {
    cordial::init_tracing();
    let roomy = resolver(vec![alias_chain(3)?], ResolveCaps::new(8, 64));
    assert_eq!(key("chain::Chain0", &roomy)?.head(), "chain::Leaf");

    let tight = resolver(vec![alias_chain(3)?], ResolveCaps::new(2, 64));
    match tight.resolve("chain::Chain0") {
        Err(Unresolved::AliasDepthExceeded(name)) => assert!(name.starts_with("Chain")),
        other => {
            return Err(miette::miette!(
                "expected the depth cap to trip, got {other:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn an_expansion_larger_than_the_node_cap_is_rejected() -> miette::Result<()> {
    cordial::init_tracing();
    let r = resolver(vec![chrono()?], ResolveCaps::new(8, 2));
    // DateTime<Utc> is 2 nodes; ParseResult<Utc> expands to 3.
    assert!(r.resolve("chrono::DateTime<chrono::Utc>").is_ok());
    assert!(matches!(
        r.resolve("chrono::ParseResult<chrono::Utc>"),
        Err(Unresolved::TypeTooLarge(_))
    ));
    Ok(())
}

#[test]
fn default_caps_match_the_documented_values() {
    cordial::init_tracing();
    let caps = ResolveCaps::default();
    assert_eq!(caps.alias_depth(), 8);
    assert_eq!(caps.max_type_nodes(), 64);
}
