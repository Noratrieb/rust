//! Implements a modified version of [RFC 3492](https://www.rfc-editor.org/rfc/rfc3492).

const BASE: u32 = 36;
const TMIN: u32 = 1;
const TMAX: u32 = 26;
const SKEW: u32 = 38;
const DAMP: u32 = 700;
const INITIAL_BIAS: u32 = 72;
const INITIAL_N: u32 = 0x80;
/// This is `-` in punycode, but we need `_` for mangling
const DELIMITER: u8 = b'_';

pub(crate) fn encode(ident: &str) -> String {
    // Position in the extended string
    let mut i;
    // Position
    let mut n = INITIAL_N;

    let mut out = String::new();

    out
}
