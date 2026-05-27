// Operator abstraction for the pattern pipeline.  GlobOperator is the trait
// that callers implement to sort, filter, or otherwise transform the list of
// matched candidates after the wildcard match step.  Two built-in
// implementations are provided: ReverseOp (reverses the result list) and
// FnOperator (wraps a closure so operators can be registered inline without
// defining a new struct).

use alloc::vec::Vec;

/// The operator trait for customizing query step transformations.
pub trait GlobOperator<T> {
    /// The operator name used for pattern matching.
    fn name(&self) -> &str;

    /// Reorders or filters the matched candidate references in-place.
    fn apply(&self, candidates: &mut Vec<&T>);
}

/// A ready-to-use operator that reverses the matched candidate list.
pub struct ReverseOp;

impl<T> GlobOperator<T> for ReverseOp {
    fn name(&self) -> &str { "REVERSE" }
    fn apply(&self, candidates: &mut Vec<&T>) {
        candidates.reverse();
    }
}

/// A generic closure-based operator wrapper.
// for<'a>: apply() receives references tied to the candidates slice —
// lifetime unknown at FnOperator construction — F must accept any 'a.
pub struct FnOperator<T, F>
where
    F: for<'a> Fn(&mut Vec<&'a T>),
{
    name: &'static str,
    func: F,
    _marker: core::marker::PhantomData<T>,
}

impl<T, F> FnOperator<T, F>
where
    F: for<'a> Fn(&mut Vec<&'a T>),
{
    /// Creates a new closure-based operator wrapper with the specified static name.
    pub fn new(name: &'static str, func: F) -> Self {
        Self {
            name,
            func,
            _marker: core::marker::PhantomData,
        }
    }
}

impl<T, F> GlobOperator<T> for FnOperator<T, F>
where
    F: for<'a> Fn(&mut Vec<&'a T>),
{
    fn name(&self) -> &str {
        self.name
    }

    fn apply(&self, candidates: &mut Vec<&T>) {
        (self.func)(candidates);
    }
}
