#![crate_type = "lib"]

macro_rules! check {
    ($e:expr) => {};

    (not $a:literal) => {};
    (NOT $a:literal) => {};
}

check! { NOT 1 } // passes (all good)
check! { not 1 } // ⚠️ FAILS but should pass! "unexpected `1` after identifier"

macro_rules! please_recover {
    ($a:expr) => {};
}

please_recover! { not 1 }