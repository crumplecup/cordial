// Same free-function name, disjoint cfg gates: `bytes` is genuinely
// read in the `not(kani)` sibling, so `_bytes` here is exempt.
#[cfg(kani)]
fn decide(_bytes: &[u8]) -> bool {
    true
}

#[cfg(not(kani))]
fn decide(bytes: &[u8]) -> bool {
    !bytes.is_empty()
}

// Same shape, but as impl methods -- exercises the impl-items path.
struct Reader;

impl Reader {
    #[cfg(kani)]
    fn read(_source: &[u8]) -> Self {
        Self
    }

    #[cfg(not(kani))]
    fn read(source: &[u8]) -> Self {
        let _ = source.len();
        Self
    }
}

// Both cfg-gated variants genuinely never use `_extra` -- neither side
// has a real, unprefixed `extra`, so this must still be flagged in both.
#[cfg(kani)]
fn unrelated(_extra: i32) -> i32 {
    1
}

#[cfg(not(kani))]
fn unrelated(_extra: i32) -> i32 {
    2
}

// A lone cfg-gated function with no same-named sibling at all -- must
// still be flagged normally.
#[cfg(kani)]
fn solo(_alone: i32) -> i32 {
    0
}
