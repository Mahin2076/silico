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
    #[arg(long, conflicts_with_all = ["live", "fixture_review", "openshell_bin", "research_query", "research_domain"])]
    hash_plan: bool,
    /// Explicitly authorize one bounded live Nemotron call through Crusoe.
    #[arg(long, conflicts_with_all = ["fixture_review", "openshell_bin"])]
    live: bool,
    /// Use an explicitly labeled model-output fixture. Never a live result.
    #[arg(long, conflicts_with_all = ["openshell_bin", "research_query", "research_domain"])]
    fixture_review: Option<PathBuf>,
    /// Run the request in an existing managed sandbox. No API key is read.
    #[arg(long)]
    openshell_bin: Option<PathBuf>,
    /// Ground contextual/methodological review in one bounded You.com search.
    /// Requires --live or --openshell-bin; never enabled implicitly.
    #[arg(long)]
    research_query: Option<String>,
    /// Restrict research to a domain. Repeat for multiple allowed domains.
    #[arg(long, requires = "research_query")]
    research_domain: Vec<String>,
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
    anyhow::ensure!(
        args.research_query.is_none() || args.live || args.openshell_bin.is_some(),
        "--research-query requires --live or --openshell-bin"
    );
    let request: VerificationRequest = serde_json::from_str(&read_bounded(&args.input, 131_072)?)?;
    if args.hash_plan {
        println!("{}", plan_hash(&request.goal, &request.plan));
        return Ok(0);
    }
    let research = args
        .research_query
        .map(|query| ResearchQuery::new(query, args.research_domain))
        .transpose()?;
    let (output, verdict) = if let Some(binary) = args.openshell_bin {
        let model = std::env::var("SILICO_VERIFIER_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.into());
        let transport = OpenShellTransport::new(binary, args.gateway, args.sandbox, model)?;
        review(transport, &request, research.as_ref()).await?
    } else if args.live {
        let transport = CrusoeTransport::from_env()?;
        review(transport, &request, research.as_ref()).await?
    } else {
        let review = args
            .fixture_review
            .as_ref()
            .map(|p| read_bounded(p, 65_536))
            .transpose()?;
        self::review(FixtureTransport { review }, &request, None).await?
    };
    let output = serde_json::to_string_pretty(&output)?;
    if let Some(path) = args.output {
        std::fs::write(path, format!("{output}\n"))?;
    } else {
        println!("{output}");
    }
    // A mock pass is useful for contract development, never a completed live run.
    Ok(if verdict == Verdict::Pass {
        0
    } else if verdict == Verdict::Repair {
        3
    } else {
        4
    })
}

async fn review<T: VerifierTransport>(
    transport: T,
    request: &VerificationRequest,
    research: Option<&ResearchQuery>,
) -> anyhow::Result<(serde_json::Value, Verdict)> {
    let verifier = Verifier::new(transport);
    if let Some(query) = research {
        // Read YOU_API_KEY only after an explicit live research request.
        let search = YouSearchClient::from_env()?;
        let result = verifier
            .verify_with_research(request, query, &search)
            .await?;
        let verdict = result.decision.verdict;
        Ok((serde_json::to_value(result)?, verdict))
    } else {
        let decision = verifier.verify(request).await;
        let verdict = decision.verdict;
        Ok((serde_json::to_value(decision)?, verdict))
    }
}
