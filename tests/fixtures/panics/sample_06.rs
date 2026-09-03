pub fn eq_token() {
    let _ = "==".parse::<proc_macro2::TokenStream>().expect("always-valid");
}
