struct ErrorType {
    message: &'static str,
    cause: &'static std::io::Error,
    location: &'static std::panic::Location<'static>,
}
impl std::error::Error for ErrorType {}
struct Ordinary {
    message: &'static str,
}
fn erased() -> Result<(), String> { todo!() }
