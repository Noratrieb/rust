#[doc(attribute = "inline")]
/// The inline attribute suggests that a copy of the attributed function should be placed
/// in the caller, rather than generating code to call the function where it is defined.
///
/// There are three ways to use the inline attribute:
/// - #[inline] suggests performing an inline expansion.
/// - #[inline(always)] suggests that an inline expansion should always be performed.
/// - #[inline(never)] suggests that an inline expansion should never be performed.
mod inline_attribute {}
