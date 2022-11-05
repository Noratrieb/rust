#![feature(no_core)]
#![feature(lang_items)]
#![no_core]
#![feature(unboxed_closures)]
#![feature(intrinsics)]
#![feature(rustc_attrs)]
#![feature(never_type)]
#[lang = "sized"]
trait Sized {}

#[lang = "copy"]
trait Copy {}

impl Copy for i32 {}

extern "Rust" {
    /// `payload` is passed through another layer of raw pointers as `&mut dyn Trait` is not
    /// FFI-safe. `BoxMeUp` lazily performs allocation only when needed (this avoids allocations
    /// when using the "abort" panic runtime).
    fn __rust_start_panic(payload: *mut ()) -> u32;
}

#[lang = "fn_once"]
pub trait FnOnce<Args> {
    /// The returned type after the call operator is used.
    #[lang = "fn_once_output"]
    type Output<'captures>;

    /// Performs the call operation.
    extern "rust-call" fn call_once(self, args: Args) -> Self::Output<'static>;
}

#[lang = "fn_mut"]
pub trait FnMut<Args>: FnOnce<Args> {
    /// Performs the call operation.
    extern "rust-call" fn call_mut<'captures>(
        &'captures mut self,
        args: Args,
    ) -> Self::Output<'captures>;
}

struct A<'a>(&'a ());

impl<'a> FnOnce<()> for A<'a> {
    type Output<'cap> = &'a ();
    extern "rust-call" fn call_once(self, args: ()) -> Self::Output<'static> {
        self.0
    }
}

struct B(());

impl FnOnce<()> for B {
    type Output<'cap> = &'cap ();
    extern "rust-call" fn call_once(self, args: ()) -> Self::Output<'static> {
        trait PostMono {
            const X: bool;
        }
        impl PostMono for () {
            const X: bool = {
                let x = 5;
                unsafe { let bool = *(&x as *const i32 as *const bool); bool }
            };
        }
        let _ = <() as PostMono>::X;
        &()
    }
}

impl FnMut<()> for B {
    extern "rust-call" fn call_mut(&mut self, args: ()) -> Self::Output<'_> {
        &self.0
    }
}

#[lang = "receiver"]
pub trait Receiver {
    // Empty.
}

impl<T: ?Sized> Receiver for &T {}
impl<T: ?Sized> Receiver for &mut T {}

#[lang = "start"]
fn lang_start<T>(_: fn() -> (), _: isize, _: *const *const u8, _: u8) -> isize {
    main();
    0
}

fn main() {
    let x = 0;
    let mut c = |a: &()| &x;

    A(&())();

    c(&());
}
