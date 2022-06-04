//! A type that stores a boxed type that derefs to a slice, and a custom index into that slice.
//!
//! The type can be mapped to have a different index, allowing you to take subslices while still
//! preserving the original type.
//!
//! This is a specialized version of `owning_ref`, because `owning_ref` has various soundness
//! issues that arise from its generality, that this one does not have.

use std::marker::PhantomData;
use std::ops::Deref;

/// An owned subslice of a boxed value.
///
/// Can be further subsliced using [`Self::map`].
pub struct OwnedSlice<O, T> {
    /// The owned value
    owned: O,
    /// The start value of the slice
    start: usize,
    /// The length of the slice
    len: usize,
    /// boo! ⊂(´･◡･⊂ )∘˚˳°
    _boo: PhantomData<*const T>,
}

impl<O, U: ?Sized, T> OwnedSlice<O, T>
where
    O: Deref<Target = U>,
    U: Deref<Target = [T]>,
{
    /// Create a new `OwnedSlice<O, T>`. Sets the subslice to contain the full slice that `O`
    /// derefs to.
    pub fn new(owned: O) -> Self {
        let len = owned.len();
        Self { owned, start: 0, len, _boo: PhantomData }
    }

    /// Change the slice to a smaller subslice, while retaining ownership over the full value.
    ///
    /// # Panics
    /// Panics if the subslice is out of bounds of the smaller subslice.
    pub fn map<F>(self, f: F) -> Self
    where
        F: FnOnce(&[T]) -> &[T],
    {
        self.try_map::<_, !>(|slice| Ok(f(slice))).unwrap()
    }

    /// Map the slice to a subslice, while retaining ownership over the full value.
    /// The function may be fallible.
    ///
    /// # Panics
    /// Panics if the returned subslice is out of bounds of the base slice.
    pub fn try_map<F, E>(self, f: F) -> Result<Self, E>
    where
        F: FnOnce(&[T]) -> Result<&[T], E>,
    {
        let base_slice: &[T] = &*self.owned;
        let std::ops::Range { start: base_ptr, end: base_end_ptr } = base_slice.as_ptr_range();
        let base_len = base_slice.len();

        let slice = &base_slice[self.start..][..self.len];
        let slice = f(slice)?;
        let (slice_ptr, len) = (slice.as_ptr(), slice.len());

        // Assert that the start pointer is in bounds, I.E. points to the same allocated object
        // If the slice is empty or contains a zero-sized type, the start and end pointers of the
        // base slice will always be the same, meaning this check will always fail.
        assert!(base_ptr <= slice_ptr);
        assert!(slice_ptr < base_end_ptr);

        // SAFETY: We have checked that the `slice_ptr` is bigger than the `base_ptr`.
        // We have also checked that it's in bounds of the allocated object.
        let diff_in_bytes = unsafe { slice_ptr.cast::<u8>().sub_ptr(base_ptr.cast::<u8>()) };

        // The subslice might not actually be a difference of sizeof(T), but truncating it should be fine.
        let start = diff_in_bytes / std::mem::size_of::<T>();

        // assert that the length is not out of bounds
        assert!((start + len) <= base_len);

        Ok(Self { owned: self.owned, start, len, _boo: PhantomData })
    }
}

impl<O, U, T> Deref for OwnedSlice<O, T>
where
    O: Deref<Target = U>,
    U: Deref<Target = [T]>,
{
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        let base_slice: &[T] = &*self.owned;
        &base_slice[self.start..][..self.len]
    }
}

/*
/// A trait like `Deref`, except it delegates to the innermost nested slice.
///
/// So `Box::<Box<Box<[u32]>>>::as_slice()` yields a `&[u32]`.
pub trait AsSlice {
    /// The type contained in the slice, often `T` for generic containers.
    type Target;

    /// Get the inner slice
    fn as_slice(&self) -> &[Self::Target];
}

// we can't have a blanket impl for O: Deref<Item = T> because that would overlap with types like box

impl<T> AsSlice for [T] {
    type Target = T;

    fn as_slice(&self) -> &[Self::Target] {
        self
    }
}
impl<T> AsSlice for Box<T>
where
    T: AsSlice<Target = T>,
{
    type Target = T;

    fn as_slice(&self) -> &[Self::Target] {
        T::as_slice(&*self)
    }
}

impl<T> AsSlice for Lrc<T>
where
    T: AsSlice<Target = T>,
{
    type Target = T;

    fn as_slice(&self) -> &[Self::Target] {
        T::as_slice(&*self)
    }
}

impl<O, T> AsSlice for OwnedSlice<O, T>
where
    O: AsSlice<Target = T>,
{
    type Target = T;

    fn as_slice(&self) -> &[Self::Target] {
        // don't delegate to the inner as_slice, we are a slice ourselves
        self.deref()
    }
}

impl<T> AsSlice for Vec<T> {
    type Target = T;

    fn as_slice(&self) -> &[Self::Target] {
        self.deref()
    }
}
*/
#[cfg(test)]
mod tests;
