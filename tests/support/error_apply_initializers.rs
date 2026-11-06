//! Constructor proof specimens, including shapes apply must reject.

const BASE: &str = r#"const TEXT: &'static str = "text";
struct Failure { file: String, message: String }
impl Failure {
    fn new<'a>(message: &'a str) -> Self {
        let loc = std::panic::Location::caller();
        let text: &'static str = "text";
        Self { file: loc.file().to_string(), message: message.to_string() }
    }
}
"#;

#[tracing::instrument(level = "trace")]
pub fn cases() -> Vec<String> {
    let mut cases = vec![BASE.to_string()];
    strings(&mut cases);
    bindings(&mut cases);
    contexts(&mut cases);
    tuples(&mut cases);
    cases
}

#[tracing::instrument(level = "trace", skip(cases))]
fn strings(cases: &mut Vec<String>) {
    for expression in [
        "message.to_string()",
        "message.to_owned()",
        "message.into()",
        "String::from(message)",
        "::std::string::String::from((message))",
        "alloc::string::String::from(message)",
        "(String::from((message)))",
        "String::from::<&str>(message)",
        "message.to_string::<()>()",
        "message.to_string(unknown)",
        "message",
        "text.to_string()",
        "self::TEXT.to_string()",
        "self::TEXT::<T>.to_string()",
        "self::MISSING.to_string()",
        "TEXT.to_string()",
        "::message.to_string()",
        "message::<T>.to_string()",
        "other::message.to_string()",
        "\"literal\".to_string()",
        "r#\"raw\"#.to_string()",
        "b\"bytes\".to_string()",
        "::std::concat!(\"a\", \"b\").to_string()",
        "::core::include_str!(\"x\").into()",
        "std::concat!(\"a\").to_string()",
        "concat!(\"a\").to_string()",
        "unknown()",
    ] {
        cases.push(BASE.replace(
            "message: message.to_string()",
            &format!("message: {expression}"),
        ));
    }
    for expression in [
        "loc.file()",
        "(loc).file().to_owned()",
        "String::from(loc.file())",
        "std::panic::Location::caller().file().to_string()",
        "Location::caller().file().into()",
        "::core::panic::Location::caller().file().to_string()",
        "Location::<T>::caller().file().to_string()",
        "(Location::caller()).file().to_string()",
        "(Location::caller)().file().to_string()",
        "loc.file::<T>().to_string()",
        "loc.file(unknown).to_string()",
        "other.file().to_string()",
        "\"\".to_string()",
        "String::new()",
    ] {
        cases.push(BASE.replace("loc.file().to_string()", expression));
    }
}

#[tracing::instrument(level = "trace", skip(cases))]
fn bindings(cases: &mut Vec<String>) {
    for binding in [
        "let loc = std::panic::Location::caller(); let loc = other();",
        "let mut loc = std::panic::Location::caller();",
        "let ref loc = std::panic::Location::caller();",
        "let loc: Location = std::panic::Location::caller();",
        "let loc = (std::panic::Location::caller());",
        "let loc = std::panic::Location::caller(); use other::Name;",
        "let loc = std::panic::Location::caller(); macro_rules! m { () => {} }",
        "let loc = std::panic::Location::caller(); m!();",
        "let loc = std::panic::Location::caller(); fn nested(loc: u32) {}",
        "let loc = std::panic::Location::caller(); impl Other { fn loc() {} }",
        "let loc = std::panic::Location::caller(); let Some(loc) = other() else { return other() };",
    ] {
        cases.push(BASE.replace("let loc = std::panic::Location::caller();", binding));
    }
    for binding in [
        "let text: &'static str = \"text\";",
        "let mut text: &'static str = \"text\";",
        "let ref text: &'static str = \"text\";",
        "let text: &'a str = message;",
        "let text: &'a mut str = message;",
        "#[cfg(unix)] let text: &'static str = \"text\";",
        "const text: &'static str = \"text\";",
        "static text: &'static str = \"text\";",
        "static mut text: &'static str = \"text\";",
        "let text: &'static str = \"text\"; { let text = other(); }",
        "let text: &'static str = \"text\"; fn text() {}",
        "let text: &'static str = \"text\"; impl Other { fn text() {} }",
        "let text: &'static str = \"text\"; use other::text;",
    ] {
        cases.push(
            BASE.replace("let text: &'static str = \"text\";", binding)
                .replace("message: message.to_string()", "message: text.to_string()"),
        );
    }
}

#[tracing::instrument(level = "trace", skip(cases))]
fn contexts(cases: &mut Vec<String>) {
    for replacement in [
        "#[cfg(unix)] file:",
        "#[cfg(unix)] #[cfg(test)] file:",
        "#[allow(dead_code)] file:",
        "/// doc\n file:",
        "#[::cfg(unix)] file:",
        "#[unsafe(cfg(unix))] file:",
    ] {
        cases.push(BASE.replace("Self { file:", &format!("Self {{ {replacement}")));
    }
    for owner in [
        "impl<'a> Failure<'a>",
        "impl<'a: 'static> Failure<'a>",
        "impl<'a, T> Failure<'a>",
        "impl<'a> ::Failure<'a>",
        "impl<'a> Failure<'b>",
        "impl<'a> Failure<'a> where Self: Copy",
        "impl other::Failure",
        "impl<'a> other::Failure<'a>",
    ] {
        cases.push(BASE.replace("impl Failure", owner));
    }
    cases.extend([
        BASE.replace("Self { file:", "Failure { file:"),
        BASE.replace("Self { file:", "other::Failure { file:"),
        BASE.replace("Self { file:", "::Failure { file:"),
        BASE.replace("Self { file:", "Failure::<T> { file:"),
        BASE.replace("message: message.to_string() }", "..other() }"),
        BASE.replace("message: message.to_string()", "message"),
        BASE.replace("fn new<'a>(message", "fn new<'a>(mut message"),
        BASE.replace("fn new<'a>(message", "fn new<'a>(ref message"),
        BASE.replace("Self { file:", "{ let message = other(); Self { file:")
            .replace("message.to_string() }", "message.to_string() } }"),
        format!("mod inner {{ {BASE} }}"),
        BASE.replace("Self { file:", "(|| Self { file:")
            .replace("message.to_string() }", "message.to_string() })()"),
        BASE.replace(
            "Self { file:",
            "{ fn nested<'a>(message: &'a str) { Self { file:",
        )
        .replace("message.to_string() }", "message.to_string() } } }"),
        BASE.replace("impl Failure", "trait Other"),
        BASE.replace("message", "mésâge")
            .replace("Failure", "Échec"),
    ]);
}

#[tracing::instrument(level = "trace", skip(cases))]
fn tuples(cases: &mut Vec<String>) {
    let tuple = r#"struct Failure { file: String }
impl Failure { fn new() -> Self {
    #[cfg(unix)] let (line, file) = { let loc = Location::caller(); (loc.line(), loc.file().to_string()) };
    #[cfg(not(unix))] let (line, file) = (0, String::new());
    Self { file }
} }
"#;
    cases.push(tuple.to_string());
    for (from, to) in [
        ("not(unix)", "not( unix )"),
        ("not(unix)", "windows"),
        ("not(unix)", "not(windows)"),
        ("not(unix)", "not(not(unix))"),
        ("not(unix)", "not(unix,)"),
        ("#[cfg(unix)]", "#[cfg(unix,)]"),
        ("unix", "true"),
        ("(0, String::new())", "(00, String::new())"),
        ("Self { file }", "Self { file: file }"),
        ("Self { file }", "Self { #[cfg(unix)] file }"),
        ("(0, String::new())", "(0, \"\")"),
        ("(0, String::new())", "(0, ::String::new())"),
        ("(0, String::new())", "(0, String::new::<T>())"),
        ("(0, String::new())", "(0, (\"\"))"),
        ("(0, String::new())", "(0x0_u32, String::new())"),
        ("(line, file)", "(line, ref file)"),
        ("(line, file)", "(line, mut file)"),
        ("Self { file }", "{ use_file(file); Self { file } }"),
        ("loc.line()", "(loc).line()"),
        ("loc.line()", "loc.line::<T>()"),
        ("#[cfg(unix)]", "#[cfg(unix)] #[cfg(test)]"),
        ("#[cfg(unix)]", "#[cfg(all(unix, feature = \"x\"))]"),
    ] {
        cases.push(tuple.replace(from, to));
    }
    for condition in [
        "all(unix, feature = \"x\")",
        "any(unix, windows)",
        "not(windows)",
        "all()",
        "any()",
    ] {
        cases.push(tuple.replace("unix", condition));
    }
    cases.push(tuple.replace("unix", "all(unix, feature = \"x\")").replace(
        "#[cfg(all(unix, feature = \"x\"))]",
        "#[cfg[all(unix, feature = \"x\")]]",
    ));
    cases.push(tuple.replace("#[cfg(unix)]", "#[cfg[unix]]"));
    cases.push(tuple.replace("#[cfg(not(unix))]", "#[cfg{not(unix)}]"));
}
