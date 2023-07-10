pub trait ConfigExt {
    /// Returns whether the value of `self` is the default value for `Self`.
    fn is_default(&self) -> bool
    where
        Self: Default + PartialEq<Self>,
    {
        *self == Self::default()
    }

    /// Returns whether the value of `self` is empty after trimming whitespace on both left and
    /// right ends.
    fn is_empty_after_trim(&self) -> bool;

    /// Returns whether the value of `self` is the default value for `Self` or empty after trimming
    /// whitespace on both left and right ends.
    fn is_default_or_empty(&self) -> bool
    where
        Self: Default + PartialEq<Self>,
    {
        self.is_default() || self.is_empty_after_trim()
    }
}

impl ConfigExt for String {
    fn is_empty_after_trim(&self) -> bool {
        self.trim().is_empty()
    }
}
