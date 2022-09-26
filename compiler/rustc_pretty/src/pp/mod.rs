//! Generic abstraction for pretty printing based on [prettyplease](https://github.com/dtolnay/prettyplease).

mod algorithm;
mod convenience;
mod iter;
mod ring;

pub use algorithm::Printer;

// Target line width.
const MARGIN: isize = 89;

// Number of spaces increment at each level of block indentation.
const INDENT: isize = 4;

// Every line is allowed at least this much space, even if highly indented.
const MIN_SPACE: isize = 60;
