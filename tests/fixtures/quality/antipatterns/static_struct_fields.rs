struct OwnsData {
    label: String,
}

struct BorrowsStatic {
    name: &'static str,
    tags: Vec<&'static str>,
}

struct TupleStatic(&'static str);

struct CapturesLocation {
    location: &'static std::panic::Location<'static>,
}

enum Message {
    Unit,
    Inline(&'static str),
    Pair(&'static str, u32),
    Named {
        code: &'static str,
        detail: String,
    },
}

mod nested {
    pub enum Payload {
        Text(&'static str),
        Rich {
            title: &'static str,
            body: String,
        },
    }
}

fn accepts(_value: &'static str) {}

fn returns() -> &'static str {
    "ok"
}
