type StringResult<T> = Result<T, String>;

fn returns_string() -> Result<(), String> {
    Ok(())
}

fn returns_str() -> Result<(), &'static str> {
    Ok(())
}

fn returns_std_string() -> Result<(), std::string::String> {
    Ok(())
}

fn ok_is_string() -> Result<String, std::io::Error> {
    Ok(String::new())
}

fn typed_error() -> Result<(), crate::error::CordialError> {
    Ok(())
}
