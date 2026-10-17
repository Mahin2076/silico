//! P2 runs independently until P1 registers `pub mod verifier` in lib.rs.
//! This path shim deliberately avoids editing P1-owned integration files.
#[allow(dead_code, unused_imports)]
#[path = "../src/verifier/mod.rs"]
mod verifier;
