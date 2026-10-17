//! Independent P2 verifier. P1 owns workflow execution and shared provider/API
//! integration. This module evaluates immutable receipts; it never mutates runs.
mod engine;
mod sandbox;
mod transport;
mod types;

pub use engine::{Verifier, VerifierLimits};
pub use sandbox::OpenShellTransport;
pub use transport::{
    Completion, CompletionFuture, CrusoeTransport, TransportError, TransportIdentity,
    VerifierTransport, DEFAULT_MODEL,
};
pub use types::*;

#[cfg(test)]
mod tests;
