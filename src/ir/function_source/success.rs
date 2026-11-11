//! Structural value exits, collected from the shared tree before planning.

use serde::{Deserialize, Serialize};

use super::SourceByteRange;

pub(super) mod native;

/// A successful exit that must acquire the lifted function's Result wrapper.
///
/// These positions belong to the enclosing source snapshot. They describe
/// syntax, not inferred types or caller closure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FunctionSuccessExit {
    /// Value evaluated once at a successful exit.
    Expression(SourceByteRange),
    /// Bare return; insert its unit operand after the keyword.
    BareReturn {
        /// Byte immediately after `return`.
        offset: usize,
    },
    /// Statement-only block falls through with unit.
    BlockUnit {
        /// Byte immediately before the block's closing brace.
        offset: usize,
    },
    /// An if without an else supplies unit on its untaken path.
    MissingElse {
        /// Byte immediately after the if expression.
        offset: usize,
    },
}
