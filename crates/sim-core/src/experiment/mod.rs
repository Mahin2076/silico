//! P1 experiment runner. Independent of the moving city simulation and legacy polls.
pub mod checks;
pub mod contract;
mod http;
pub mod p2;
mod runner;
mod store;
#[cfg(test)]
mod tests;
pub use http::router;
use sha2::{Digest, Sha256};
fn hash(value: &impl serde::Serialize) -> String {
    hex::encode(Sha256::digest(
        serde_json::to_vec(value).expect("serializable experiment input"),
    ))
}
