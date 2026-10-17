//! Independent P2 verifier. P1 owns workflow execution and shared provider/API
//! integration. This module evaluates immutable receipts; it never mutates runs.
mod claims;
mod engine;
mod repairs;
mod research;
mod sandbox;
mod transport;
mod types;

pub use claims::{ClaimAssessment, ClaimEvidence, ClaimKind, ComparisonProof, ComparisonRelation};
pub use engine::{Verifier, VerifierLimits};
pub use repairs::{RepairAssessment, RepairContext, RepairObligation};
pub use research::{
    ResearchQuery, ResearchedVerification, WebResearch, WebSource, YouSearchClient,
};
pub use sandbox::OpenShellTransport;
pub use transport::{
    Completion, CompletionFuture, CrusoeTransport, TransportError, TransportIdentity,
    VerifierTransport, DEFAULT_MODEL,
};
pub use types::*;

#[cfg(test)]
mod claim_tests;
#[cfg(test)]
mod repair_tests;
#[cfg(test)]
mod research_tests;
#[cfg(test)]
mod tests;
