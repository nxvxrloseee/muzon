//! Test fixtures, moved to `muzon-core` so the native player's tests and the
//! desktop app's tests use the same tones, scratch directories and fake HTTP
//! server. Test-only from this crate's point of view: `mod testing` is still
//! `#[cfg(test)]` in `lib.rs`.

pub use muzon_core::testing::*;
