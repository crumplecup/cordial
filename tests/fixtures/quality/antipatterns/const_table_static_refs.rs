struct ConstRow {
    name: &'static str,
    tags: &'static [&'static str],
}

const ROWS: &[ConstRow] = &[ConstRow {
    name: "alpha",
    tags: &["a"],
}];

static ALSO: ConstRow = ConstRow {
    name: "beta",
    tags: &[],
};

struct RuntimeRow {
    name: &'static str,
}

fn build() -> RuntimeRow {
    RuntimeRow { name: "gamma" }
}

struct MixedUse {
    name: &'static str,
}

const MIXED: MixedUse = MixedUse { name: "const" };

fn also_mixed() -> MixedUse {
    MixedUse { name: "runtime" }
}

struct ConstLoc {
    location: &'static std::panic::Location<'static>,
}

const LOC: ConstLoc = ConstLoc {
    location: DUMMY,
};
