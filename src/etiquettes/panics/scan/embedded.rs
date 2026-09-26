use syn::visit::Visit;

use super::snippets::looks_like_embedded_panic_source;
use super::{PanicScanVisitor, PanicSiteRecord, build_kani_reachability};

use tracing::instrument;

impl PanicScanVisitor<'_> {
    /// A string that parses as a Rust file and contains abort APIs is a
    /// fixture program smuggled into the scanning crate. Scan it with its
    /// own Kani/Verus reachability so the findings are the same as if the
    /// program lived in a real `.rs` file, then map lines back onto the
    /// literal in the outer file.
    #[instrument(level = "debug", skip(self, lit))]
    pub(super) fn scan_embedded_source(&mut self, lit: &syn::LitStr) {
        if self.in_embedded_source {
            return;
        }
        let value = lit.value();
        if !looks_like_embedded_panic_source(&value) {
            return;
        }
        let origin = lit.span().start().line as u32;
        if let Ok(syntax) = syn::parse_file(&value) {
            self.scan_embedded_syn_file(origin, syntax);
        }
        #[cfg(feature = "verus_ir")]
        self.scan_embedded_verus_source(origin, &value);
    }

    #[instrument(level = "debug", skip(self, syntax))]
    fn scan_embedded_syn_file(&mut self, origin: u32, syntax: syn::File) {
        let inner_reach = build_kani_reachability(std::iter::once(&syntax));
        let mut nested = PanicScanVisitor {
            file: self.file.clone(),
            crate_root: self.crate_root.clone(),
            module_prefix: self.module_prefix.clone(),
            impl_type: self.impl_type.clone(),
            fn_stack: self.fn_stack.clone(),
            in_cfg_test: self.in_cfg_test,
            in_cfg_not_kani: false,
            reachability: &inner_reach,
            findings: Vec::new(),
            exempt_error_assertion_lines: std::collections::HashSet::new(),
            in_embedded_source: true,
            error: None,
        };
        nested.visit_file(&syntax);
        if let Some(error) = nested.error {
            self.error = Some(error);
            return;
        }
        for finding in nested.findings {
            let cfg_test = finding.cfg_test();
            self.push_adjusted_embedded_finding(origin, finding, cfg_test);
        }
    }

    #[cfg(feature = "verus_ir")]
    #[instrument(level = "debug", skip(self, value))]
    fn scan_embedded_verus_source(&mut self, origin: u32, value: &str) {
        let module_path = self.site_context();
        let ir = match crate::verus_ir::scan_verus_rust_source(value, &self.file, &module_path) {
            Ok(ir) => ir,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let records = match super::verus_panics::findings(&ir, &self.crate_root) {
            Ok(records) => records,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        for record in records {
            self.push_adjusted_embedded_finding(origin, record, self.in_cfg_test);
        }
    }

    #[instrument(level = "trace", skip(self, finding))]
    fn push_adjusted_embedded_finding(
        &mut self,
        origin: u32,
        finding: PanicSiteRecord,
        cfg_test: bool,
    ) {
        if self.error.is_some() {
            return;
        }
        match PanicSiteRecord::builder()
            .kind(finding.kind())
            .context(finding.context().clone())
            .file(finding.file().clone())
            .line(origin.saturating_add(finding.line().saturating_sub(1)))
            .snippet(finding.snippet().clone())
            .cfg_test(cfg_test)
            .build()
        {
            Ok(record) => self.findings.push(record),
            Err(error) => self.error = Some(error),
        }
    }
}
