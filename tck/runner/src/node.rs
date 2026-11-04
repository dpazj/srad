//! Management of the process running the node under test.

use std::{fs::File, path::Path, process::Stdio, time::Duration};

use anyhow::{Context, Result};

pub struct NodeProcess {
    child: tokio::process::Child,
}

impl NodeProcess {
    /// Spawn `command` through `sh -c`, redirecting its output to `log_path`.
    ///
    /// The child is started in its own process group so that a graceful
    /// `SIGINT` reaches the whole tree (e.g. `cargo run` and the example it
    /// spawns). `envs` are added to the child's environment.
    pub fn spawn(command: &str, log_path: &Path, envs: &[(&str, String)]) -> Result<Self> {
        let log = File::create(log_path)
            .with_context(|| format!("creating node log {}", log_path.display()))?;

        let mut std_command = std::process::Command::new("sh");
        std_command
            .arg("-c")
            .arg(command)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log))
            .envs(envs.iter().map(|(key, value)| (*key, value.as_str())));

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt as _;
            std_command.process_group(0);
        }

        let child = tokio::process::Command::from(std_command)
            .spawn()
            .with_context(|| format!("spawning node command `{command}`"))?;

        Ok(Self { child })
    }

    /// Signal the node to shut down and wait for it to exit. Falls back to
    /// `SIGKILL` if it does not exit in time.
    pub async fn stop(&mut self, timeout: Duration) {
        if let Ok(Some(_)) = self.child.try_wait() {
            return;
        }

        #[cfg(unix)]
        if let Some(pid) = self.child.id() {
            // Negative pid signals the whole process group.
            unsafe {
                libc::kill(-(pid as i32), libc::SIGINT);
            }
        }

        match tokio::time::timeout(timeout, self.child.wait()).await {
            Ok(_) => {}
            Err(_) => self.kill().await,
        }
    }

    async fn kill(&mut self) {
        #[cfg(unix)]
        if let Some(pid) = self.child.id() {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }

        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
    }
}

impl Drop for NodeProcess {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            #[cfg(unix)]
            if let Some(pid) = self.child.id() {
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGKILL);
                }
            }
            let _ = self.child.start_kill();
        }
    }
}
