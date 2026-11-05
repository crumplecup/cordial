//! Keep the feature-gated error evidence interface in one module.
//! The macro inserts methods into IrView without changing its public API.

macro_rules! required_readers {
    () => {
        /// Read required error-apply declarations supplied by the source loader.
        #[instrument(level = "trace", skip(self), err(level = "warn"))]
        fn source_error_apply(&self) -> CordialResult<crate::ir::SourceErrorApply> {
            evidence::required(
                self,
                crate::ir::ATTR_SOURCE_ERROR_APPLY,
                "source error-apply inventory",
            )
        }

        /// Read required architecture contracts without source or private caches.
        #[instrument(level = "trace", skip(self), err(level = "warn"))]
        fn source_error_architecture(&self) -> CordialResult<crate::ir::SourceErrorArchitecture> {
            evidence::required(
                self,
                crate::ir::ATTR_SOURCE_ERROR_ARCHITECTURE,
                "source error-architecture inventory",
            )
        }

        /// Read required conversion signatures without source or private caches.
        #[instrument(level = "trace", skip(self), err(level = "warn"))]
        fn source_error_signatures(&self) -> CordialResult<crate::ir::SourceErrorSignatures> {
            evidence::required(
                self,
                crate::ir::ATTR_SOURCE_ERROR_SIGNATURES,
                "source error-signature inventory",
            )
        }

        /// Read required compliance evidence without syntax or private caches.
        #[instrument(level = "trace", skip(self), err(level = "warn"))]
        fn source_error_compliance(&self) -> CordialResult<crate::ir::SourceErrorCompliance> {
            evidence::required(
                self,
                crate::ir::ATTR_SOURCE_ERROR_COMPLIANCE,
                "source error-compliance inventory",
            )
        }

        /// Read required type relationships without syntax or private caches.
        #[instrument(level = "trace", skip(self), err(level = "warn"))]
        fn source_error_types(&self) -> CordialResult<crate::ir::SourceErrorTypes> {
            evidence::required(
                self,
                crate::ir::ATTR_SOURCE_ERROR_TYPES,
                "source error-type inventory",
            )
        }
    };
}

pub(super) use required_readers;
