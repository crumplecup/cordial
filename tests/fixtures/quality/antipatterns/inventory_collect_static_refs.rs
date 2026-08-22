struct Registered {
    name: &'static str,
}

inventory::collect!(Registered);

struct NotRegistered {
    name: &'static str,
}

fn build() -> NotRegistered {
    NotRegistered { name: "runtime" }
}
