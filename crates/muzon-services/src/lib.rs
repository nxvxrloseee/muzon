//! Explicit network integrations. Constructors never contact a service.
#![forbid(unsafe_code)]
pub mod data;
pub mod domain;
pub mod hub;
#[cfg(test)]
pub use muzon_core::testing;

/// Input fields and asynchronous messages never format credential values.
#[derive(Clone)]
pub struct Secret(pub String);
impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[redacted]")
    }
}
impl Drop for Secret {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.0.zeroize();
    }
}
