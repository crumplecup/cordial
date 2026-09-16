use proc_macro2::{Delimiter, Group, TokenStream, TokenTree};
use tracing::instrument;

#[instrument(level = "debug", skip(tokens))]
pub(super) fn collect_functions(
    tokens: TokenStream,
) -> Vec<super::super::verus_recover::VerusFunctionChunk> {
    super::super::verus_recover::collect_verus_functions(tokens)
}

impl super::PanicScanVisitor<'_> {
    /// Parse each recovered `verus! { .. }` function chunk as a real
    /// `syn::Block` and visit it with this same visitor -- `fn_stack`
    /// gets the chunk's real name pushed first, so findings inside get
    /// the same accurate context (and the same Kani-reachability/
    /// cfg(test) handling) as an ordinary function would. A chunk that
    /// fails to parse (genuine Verus-only expression syntax -- see
    /// `verus_recover`'s own doc comment) is retried one level deeper
    /// via `collect_verus_functions` on its own body tokens, to still
    /// opportunistically find a nested `fn` even inside an outer shell
    /// this scanner can't fully make sense of. Only compiled in when
    /// `verus_ir` (a genuinely complete parse) isn't available.
    #[instrument(level = "debug", skip(self, chunks))]
    pub(super) fn scan_verus_chunks(
        &mut self,
        chunks: Vec<super::super::verus_recover::VerusFunctionChunk>,
    ) {
        for chunk in chunks {
            let (name, body) = chunk.into_parts();
            let braced =
                TokenStream::from(TokenTree::Group(Group::new(Delimiter::Brace, body.clone())));
            match syn::parse2::<syn::Block>(braced) {
                Ok(block) => {
                    self.fn_stack.push(name);
                    syn::visit::visit_block(self, &block);
                    self.fn_stack.pop();
                }
                Err(_) => {
                    self.scan_verus_chunks(collect_functions(body));
                }
            }
        }
    }
}
