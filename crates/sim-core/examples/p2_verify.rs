//! Standalone P2 harness. It intentionally does not register routes, change the
//! shared model client, or edit P1's lib.rs. All fixtures are explicitly labeled.
#[allow(dead_code, unused_imports)]
#[path = "../src/verifier/mod.rs"]
mod verifier;

use clap::Parser;
use std::{io::Read, path::PathBuf};
use verifier::*;

#[derive(Parser)]
#[command(about = "Silico P2: independently verify immutable experiment evidence")]
struct Args {
    #[arg(long)]
    input: PathBuf,
    /// Print the canonical immutable-plan hash without calling a provider.
    #[arg(long, conflicts_with_all = ["live", "fixture_review", "openshell_bin"])]
    hash_plan: bool,
    /// Explicitly authorize one bounded live Nemotron call through Crusoe.
    #[arg(long, conflicts_with_all = ["fixture_review", "openshell_bin"])]
    live: bool,
    /// Use an explicitly labeled model-output fixture. Never a live result.
    #[arg(long, conflicts_with = "openshell_bin")]
    fixture_review: Option<PathBuf>,
    /// Run the request in an existing managed sandbox. No API key is read.
    #[arg(long)]
    openshell_bin: Option<PathBuf>,
    #[arg(long, default_value = "nemoclaw")]
    gateway: String,
    #[arg(long, default_value = "simtra")]
    sandbox: String,
    #[arg(long)]
    output: Option<PathBuf>,
}

struct FixtureTransport {
    review: Option<String>,
}
impl VerifierTransport for FixtureTransport {
    fn identity(&self) -> TransportIdentity {
        TransportIdentity {
            mode: "mock".into(),
            provider: "explicit-fixture".into(),
            model: DEFAULT_MODEL.into(),
        }
    }
    fn complete<'a>(&'a self, _: &'a str, _: &'a str) -> CompletionFuture<'a> {
        Box::pin(async move {
            self.review
                .as_ref()
                .map(|text| Completion {
                    text: text.clone(),
                    model: DEFAULT_MODEL.into(),
                })
                .ok_or(TransportError::MissingConfiguration)
        })
    }
}

fn read_bounded(path: &PathBuf, limit: u64) -> anyhow::Result<String> {
    let mut text = String::new();
    if path.as_os_str() == "-" {
        std::io::stdin().take(limit + 1).read_to_string(&mut text)?;
    } else {
        std::fs::File::open(path)?
            .take(limit + 1)
            .read_to_string(&mut text)?;
    }
    anyhow::ensure!(
        text.len() as u64 <= limit,
        "Input exceeds the {} byte limit",
        limit
    );
    Ok(text)
}

#[tokio::main]
async fn main() {
    match run(Args::parse()).await {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("P2 verifier could not run: {error}");
            std::process::exit(2);
        }
    }
}

async fn run(args: Args) -> anyhow::Result<i32> {
    let request: VerificationRequest = serde_json::from_str(&read_bounded(&args.input, 131_072)?)?;
    if args.hash_plan {
        println!("{}", plan_hash(&request.goal, &request.plan));
        return Ok(0);
    }
    let decision = if let Some(binary) = args.openshell_bin {
        let model = std::env::var("SILICO_VERIFIER_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.into());
        let transport = OpenShellTransport::new(binary, args.gateway, args.sandbox, model)?;
        Verifier::new(transport).verify(&request).await
    } else if args.live {
        let transport = CrusoeTransport::from_env()?;
        Verifier::new(transport).verify(&request).await
    } else {
        let review = args
            .fixture_review
            .as_ref()
            .map(|p| read_bounded(p, 65_536))
            .transpose()?;
        Verifier::new(FixtureTransport { review })
            .verify(&request)
            .await
    };
    let output = serde_json::to_string_pretty(&decision)?;
    if let Some(path) = args.output {
        std::fs::write(path, format!("{output}\n"))?;
    } else {
        println!("{output}");
    }
    // A mock pass is useful for contract development, never a completed live run.
    Ok(if decision.verdict == Verdict::Pass {
        0
    } else if decision.verdict == Verdict::Repair {
        3
    } else {
        4
    })
}
