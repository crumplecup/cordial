pub fn ok() {}

#[cfg(test)]
mod tests {
    #[test]
    fn tmp() { let _ = Some(1).unwrap(); }
}
