//! Isolated return-lifting specimen: u32 → Result; a later transform supplies the error type.

fn value(early: bool) -> u32 {
    if early {
        return 7;
    }
    11
}

fn main() {
    let early = std::env::args().any(|argument| argument == "early");
    println!("{:?}", value(early));
}
