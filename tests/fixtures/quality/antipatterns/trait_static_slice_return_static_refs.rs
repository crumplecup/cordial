struct Transition {
    name: &'static str,
}

trait StateMachine {
    fn transitions() -> &'static [Transition];

    fn root(&self) -> &'static Transition;

    fn root_entries() -> &'static [Transition] {
        &[]
    }
}

struct NotPromised {
    name: &'static str,
}

fn build() -> NotPromised {
    NotPromised { name: "runtime" }
}
