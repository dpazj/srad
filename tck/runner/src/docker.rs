//! Optional lifecycle management for the containerised TCK broker.

use std::{
    process::Stdio,
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use tokio::process::Command;

pub struct Container {
    name: String,
    running: bool,
}

impl Container {
    /// Start the TCK image in headless mode. `TCK_HEADLESS=1` tells the image
    /// to run HiveMQ only, without the web console.
    pub async fn start(image: &str, name: &str) -> Result<Self> {
        // Remove a container left behind by an interrupted run.
        let _ = docker(&["rm", "--force", name]).await;

        const ATTEMPTS: usize = 10;
        for attempt in 1..=ATTEMPTS {
            let output = docker(&[
                "run",
                "--detach",
                "--rm",
                "--name",
                name,
                "--network",
                "host",
                "--env",
                "TCK_HEADLESS=1",
                image,
            ])
            .await
            .context("running docker; is the docker CLI installed and the daemon running?")?;

            if output.status.success() {
                return Ok(Self {
                    name: name.to_string(),
                    running: true,
                });
            }

            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            // `--rm` removal of the previous container is asynchronous, so a
            // container with the same name can linger for a moment.
            if stderr.contains("already in use") && attempt < ATTEMPTS {
                tokio::time::sleep(Duration::from_millis(300)).await;
                continue;
            }

            bail!("failed to start the TCK container:\n{stderr}");
        }

        unreachable!("the retry loop always returns or bails")
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Wait until the broker accepts TCP connections.
    pub async fn wait_ready(host: &str, port: u16, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            let address = format!("{host}:{port}");
            if tokio::net::TcpStream::connect(&address).await.is_ok() {
                // Give the TCK extension a moment to finish starting up.
                tokio::time::sleep(Duration::from_millis(500)).await;
                return Ok(());
            }

            if Instant::now() >= deadline {
                bail!("timed out waiting for the TCK broker on {address}");
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    pub async fn stop(mut self) {
        self.running = false;
        self.remove().await;
    }

    async fn remove(&self) {
        // Stop and remove explicitly instead of relying on `--rm`: the
        // removal must have finished before the next container starts.
        let _ = docker(&["stop", "--time", "5", &self.name]).await;
        let _ = docker(&["rm", "--force", &self.name]).await;
    }
}

impl Drop for Container {
    fn drop(&mut self) {
        if !self.running {
            return;
        }
        // Best effort: don't leave containers behind if the runner panics.
        let _ = std::process::Command::new("docker")
            .args(["rm", "--force", &self.name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
}

async fn docker(args: &[&str]) -> Result<std::process::Output> {
    Command::new("docker")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .with_context(|| format!("running `docker {}`", args.join(" ")))
}
