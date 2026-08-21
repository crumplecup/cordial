// Compiler-mandated proc-macro ABI: the parameter list isn't the
// author's to shrink, same reasoning as a foreign trait impl.
#[proc_macro_attribute]
pub fn my_attr(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_derive(MyDerive)]
pub fn my_derive(_input: TokenStream) -> TokenStream {
    TokenStream::new()
}

#[proc_macro]
pub fn my_macro(_input: TokenStream) -> TokenStream {
    TokenStream::new()
}

// Creusot's real idiom: #[logic(opaque)] + a bare `dead` body. The
// parameter makes the axiom parametric across call sites; the body must
// never read it by design.
#[trusted]
#[logic(opaque)]
fn opaque_len(_s: &String) -> usize {
    dead
}

// #[logic(opaque)] present, but a real body -- not the `dead` idiom, so
// still flagged.
#[trusted]
#[logic(opaque)]
fn opaque_len_with_real_body(_s: &String) -> usize {
    0
}

// Body is `dead`, but no #[logic(opaque)] attribute -- a coincidental
// identifier, not the real idiom, so still flagged.
fn looks_like_dead_but_is_not(_s: &String) -> usize {
    dead
}
