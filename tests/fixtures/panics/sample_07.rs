fn take(map: &std::collections::HashMap<String, String>) -> String {
    map.get("k").unwrap().as_str().unwrap().to_string()
}
