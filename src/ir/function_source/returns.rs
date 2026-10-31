//! Return paths and boundaries collected during the shared source walk.

use serde::{Deserialize, Serialize};

use super::{FunctionSuccessExit, SourceByteRange};

mod compatibility;
mod native;

/// A return exits the current function, rather than a nested execution scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct FunctionReturn {
    /// Whole return expression, including its keyword.
    range: SourceByteRange,
    /// A bare return produces unit; otherwise retain the exact value range.
    value: ReturnValue,
}

/// Value returned explicitly from a function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReturnValue {
    /// `return` without an operand.
    Unit,
    /// Operand evaluated once when this return is reached.
    Expression(SourceByteRange),
}

/// Surface syntax at the end of a function body.
///
/// Structural success exits separately describe which paths can supply a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FunctionTail {
    /// Empty or statement-ending block; this alone does not prove fallthrough.
    Statements,
    /// An explicit return is already represented in the return inventory.
    Return,
    /// A trailing expression supplies the value.
    Expression(SourceByteRange),
}

/// Syntax requiring additional proof before changing a function contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FunctionBoundary {
    /// Externally visible callers have not been inventoried.
    Visible,
    /// Const, async, unsafe, ABI, or variadic contracts need dedicated handling.
    SpecialSignature,
    /// Generic contracts need type and lifetime substitution evidence.
    Generic,
    /// Attributes may impose cfg, macro, or external signature contracts.
    Attribute,
    /// Macro expansion may contain additional exits or depend on return types.
    Macro,
    /// Existing residual propagation needs a resolved conversion contract.
    Try,
    /// Nested scopes have separate returns and capture contracts.
    NestedScope,
    /// Loop and break targets need a dedicated control-flow contract.
    LoopControl,
    /// The never type cannot be used as a stable generic Result argument.
    NeverOutput,
}

/// A nested body whose returns do not belong to the enclosing function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct FunctionNestedScope {
    range: SourceByteRange,
    #[getter(copy)]
    kind: NestedScopeKind,
}

/// Execution boundaries within a function body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NestedScopeKind {
    /// Closure, with its own return contract.
    Closure,
    /// Async block, returning into its future.
    Async,
    /// Local item, whose body is not executed as part of the enclosing body.
    Item,
    /// Inline const evaluation has a distinct execution context.
    Const,
}

/// Complete syntactic return inventory for one collected function body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct FunctionReturnFlow {
    returns: Vec<FunctionReturn>,
    tail: FunctionTail,
    success_exits: Vec<FunctionSuccessExit>,
    nested_scopes: Vec<FunctionNestedScope>,
    boundaries: Vec<FunctionBoundary>,
}
