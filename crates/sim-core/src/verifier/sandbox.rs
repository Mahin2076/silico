//! Optional contained request execution. The model and evidence cross stdin;
//! credentials stay in the existing OpenShell gateway. No shell commands or
//! repair actions are accepted from the model.
use super::{
    transport::{decode_completion, request_body, MAX_RESPONSE_BYTES},
    *,
};
use std::{path::PathBuf, process::Stdio};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const BRIDGE: &str = include_str!("sandbox_bridge.py");

#[derive(Debug)]
pub struct OpenShellTransport {
    executable: PathBuf,
    gateway: String,
    sandbox: String,
    model: String,
    mode: String,
}

impl OpenShellTransport {
    /// Requires an existing Ready sandbox with the managed inference route,
    /// Python 3, and policy permission for Python to call inference.local.
    /// This never creates sandboxes or changes network policy automatically.
    pub fn new(
        executable: PathBuf,
        gateway: String,
        sandbox: String,
        model: String,
    ) -> Result<Self, TransportError> {
        fn runtime_id(value: &str) -> bool {
            (1..=64).contains(&value.len())
                && value
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        }
        if !executable.is_absolute()
            || !executable.is_file()
            || !runtime_id(&gateway)
            || !runtime_id(&sandbox)
            || !model.starts_with("nvidia/")
            || !model.to_ascii_lowercase().contains("nemotron")
            || model.len() > 160
        {
            return Err(TransportError::InvalidConfiguration);
        }
        Ok(Self {
            executable,
            gateway,
            sandbox,
            model,
            mode: "live".into(),
        })
    }

    #[cfg(test)]
    pub(crate) fn for_test(executable: PathBuf) -> Self {
        let mut transport = Self::new(
            executable,
            "fixture".into(),
            "fixture".into(),
            DEFAULT_MODEL.into(),
        )
        .unwrap();
        transport.mode = "mock".into();
        transport
    }
}

impl VerifierTransport for OpenShellTransport {
    fn identity(&self) -> TransportIdentity {
        TransportIdentity {
            mode: self.mode.clone(),
            provider: "crusoe-via-openshell".into(),
            model: self.model.clone(),
        }
    }

    fn complete<'a>(&'a self, system: &'a str, user: &'a str) -> CompletionFuture<'a> {
        Box::pin(async move {
            let payload = request_body(&self.model, system, user).to_string();
            if payload.len() > 262_144 {
                return Err(TransportError::InvalidConfiguration);
            }
            let mut child = tokio::process::Command::new(&self.executable)
                .args([
                    "sandbox",
                    "exec",
                    "--gateway",
                    &self.gateway,
                    "--name",
                    &self.sandbox,
                    "--timeout",
                    "30",
                    "--no-tty",
                    "--",
                    "/usr/bin/python3",
                    "-c",
                    BRIDGE,
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .map_err(|_| TransportError::Network)?;
            let mut stdin = child.stdin.take().ok_or(TransportError::Network)?;
            stdin
                .write_all(payload.as_bytes())
                .await
                .map_err(|_| TransportError::Network)?;
            stdin
                .shutdown()
                .await
                .map_err(|_| TransportError::Network)?;
            drop(stdin);
            let mut bytes = Vec::new();
            child
                .stdout
                .take()
                .ok_or(TransportError::Network)?
                .take(MAX_RESPONSE_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
                .await
                .map_err(|_| TransportError::Network)?;
            if bytes.len() > MAX_RESPONSE_BYTES {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(TransportError::ResponseTooLarge);
            }
            let status = child.wait().await.map_err(|_| TransportError::Network)?;
            // The fixed bridge returns only sanitized error categories. Never
            // forward a subprocess error body or an upstream response verbatim.
            if let Ok(error) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                if let Some(code) = error["p2_bridge_error"].as_str() {
                    return Err(match code {
                        "http" => error["status"]
                            .as_u64()
                            .filter(|s| (100..=599).contains(s))
                            .map(|s| TransportError::HttpStatus(s as u16))
                            .unwrap_or(TransportError::Network),
                        "timeout" => TransportError::Timeout,
                        "too_large" => TransportError::ResponseTooLarge,
                        "invalid_input" => TransportError::InvalidConfiguration,
                        _ => TransportError::Network,
                    });
                }
            }
            if !status.success() {
                return Err(TransportError::Network);
            }
            decode_completion(&bytes, &self.model)
        })
    }
}
