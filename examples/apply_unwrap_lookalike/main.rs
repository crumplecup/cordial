//! Rejection input: spelling `unwrap` does not establish a Result contract.

struct OrdinaryValue;

impl OrdinaryValue {
    fn unwrap(self) -> u32 {
        7
    }
}

fn leaf() -> u32 {
    OrdinaryValue.unwrap()
}

fn main() {
    println!("{}", leaf());
}
