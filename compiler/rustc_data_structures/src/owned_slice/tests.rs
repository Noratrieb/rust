use std::ops::Deref;

use super::OwnedSlice;

/*
#[test]
fn create_deref() {
    let box_slice = OwnedSlice::new(Box::new(vec![1, 2, 3]));

    let slice = &*box_slice;

    assert_eq!(slice, &[1, 2, 3]);
}
*/
#[test]
fn map() {
    let owned_slice: OwnedSlice<Box<dyn Deref<Target = [u8]>>, _> =
        OwnedSlice::new(Box::new(vec![1, 2, 3, 4, 5]));

    let owned_slice = owned_slice.map(|slice| slice.split_at(2).1);
    let slice = &*owned_slice;

    assert_eq!(slice, &[3, 4, 5]);
}
/*
#[test]
#[should_panic]
fn out_of_bounds() {
    static X: [u8; 5] = [1, 2, 3, 4, 5];

    let box_slice = OwnedSlice::new(Box::new(vec![1u8, 2, 3]));
    let box_slice = box_slice.map(|_| &X[..]);
    let slice = &*box_slice;

    assert_eq!(slice, &[1, 2, 3, 4, 5]);
}

#[test]
#[should_panic]
fn no_zsts_allowed() {
    let other = Box::leak(Box::new(vec![(); 5]));
    ignore_leak(other);

    let box_slice = OwnedSlice::new(vec![(); 5]);
    let box_slice = box_slice.map(|_| &other[..]);
    let slice = &*box_slice;

    assert_eq!(slice, other);
}

#[test]
#[should_panic]
fn empty_slices() {
    let other = Box::leak(Box::new(vec![0u8; 0]));
    ignore_leak(other);

    let box_slice = OwnedSlice::new(vec![]);
    let box_slice = box_slice.map(|_| &other[..]);
    let slice = &*box_slice;

    assert_eq!(slice, &[]);
}*/

fn ignore_leak<T>(_ptr: *const T) {
    #[cfg(miri)]
    extern "Rust" {
        fn miri_static_root(ptr: *const u8);
    }
    #[cfg(miri)]
    unsafe {
        miri_static_root(_ptr.cast())
    };
}
