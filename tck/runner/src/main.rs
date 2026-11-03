//! Programmatic runner for the Eclipse Sparkplug TCK.
//!
//! The TCK is controlled entirely over MQTT, so the web console is not
//! required. This runner starts tests, optionally manages the broker and the
//! node under test, collects the results and emits machine readable reports.

mod catalog;
mod docker;
mod driver;
mod node;
mod report;

use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use tokio::time::{sleep, Instant};

use catalog::{NodeMode, Profile, TestParams, TestSpec};
use docker::Container;
use driver::{Driver, Message};
use node::NodeProcess;
use report::{Outcome, Status, TestResult};

#[derive(Debug, Parser)]
#[command(
    name = "tck-runner",
    version,
    about = "Run the Eclipse Sparkplug TCK programmatically",
    long_about = "Run the Eclipse Sparkplug TCK programmatically.\n\n\
                  Tests are selected with `profile:name` (for example `edge:SendDataTest`) or \
                  with `--suite`. The runner talks to the TCK over MQTT; it can start the \
                  containerised TCK broker and the node under test for you, and writes JUnit \
                  XML and JSON reports.\n\n\
                  Examples:\n  \
                  tck-runner edge:SendDataTest --node-cmd ./target/debug/examples/simple-node\n  \
                  tck-runner --suite edge --docker-image srad-tck --node-cmd ./target/debug/examples/simple-node\n  \
                  tck-runner --suite broker --docker-image srad-tck\n  \
                  tck-runner --list"
)]
struct Cli {
    /// Tests to run, as `<profile>:<test>` (e.g. `edge:SendDataTest`).
    /// Overrides `--suite`.
    #[arg(value_name = "PROFILE:TEST")]
    tests: Vec<String>,

    /// Run every test of a profile.
    #[arg(long, value_enum, default_value_t = Suite::Edge)]
    suite: Suite,

    /// List the available tests and exit.
    #[arg(long)]
    list: bool,

    /// MQTT broker address, as `host:port`.
    #[arg(long, default_value = "127.0.0.1:1883")]
    broker: String,

    /// Host application id used by the tests.
    #[arg(long, default_value = "SCADA_HOST")]
    host_app_id: String,

    /// Edge node group id.
    #[arg(long, default_value = "foo")]
    group_id: String,

    /// Edge node id.
    #[arg(long, default_value = "bar")]
    node_id: String,

    /// Device id; repeat for tests that take multiple devices.
    #[arg(long = "device-id", default_value = "dev1")]
    device_ids: Vec<String>,

    /// MQTT client id of the host application under test.
    #[arg(long, default_value = "srad-app")]
    client_id: String,

    /// URI of the second broker used by the multiple broker tests.
    #[arg(long, default_value = "tcp://localhost:1884")]
    second_broker: String,

    /// Reorder timeout in milliseconds for the host message ordering test.
    #[arg(long, default_value_t = 5000)]
    reorder_timeout: u64,

    /// Per-test timeout in seconds.
    #[arg(long, default_value_t = 120)]
    timeout: u64,

    /// UTC window in milliseconds reported to the TCK.
    #[arg(long, default_value_t = 60000)]
    utc_window: u64,

    /// Command that starts the node under test, run through `sh -c`.
    #[arg(long)]
    node_cmd: Option<String>,

    /// Delay in milliseconds before starting or after starting the node.
    #[arg(long, default_value_t = 1000)]
    node_start_delay: u64,

    /// Delay in milliseconds before stopping the node in termination tests.
    #[arg(long, default_value_t = 5000)]
    node_stop_delay: u64,

    /// Automatic reply to interactive TCK prompts, e.g. `PASS` or `FAIL`.
    #[arg(long)]
    console_reply: Option<String>,

    /// Directory for logs and reports.
    #[arg(long, default_value = "tck/results")]
    results_dir: PathBuf,

    /// Write the JUnit XML report to this path.
    #[arg(long)]
    junit: Option<PathBuf>,

    /// Write the JSON report to this path.
    #[arg(long)]
    json: Option<PathBuf>,

    /// Start the containerised TCK broker from this image.
    #[arg(long)]
    docker_image: Option<String>,

    /// Name of the TCK container.
    #[arg(long, default_value = "srad-tck")]
    docker_name: String,

    /// Reuse one container for the whole run (`shared`) or start a fresh one
    /// for every test (`per-test`). Isolation matters for tests that inspect
    /// bdSeq increments across node restarts.
    #[arg(long, value_enum, default_value_t = Isolation::PerTest)]
    docker_isolation: Isolation,

    /// Timeout in seconds for the broker container to become ready.
    #[arg(long, default_value_t = 60)]
    startup_timeout: u64,

    /// Leave the container running after the run finishes.
    #[arg(long)]
    no_stop: bool,

    /// Print TCK log messages while tests run.
    #[arg(long, short)]
    verbose: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Suite {
    Edge,
    Host,
    Broker,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Isolation {
    /// Reuse one container for the whole run.
    Shared,
    /// Start a fresh container for every test.
    PerTest,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("error: {error:#}");
            std::process::exit(2);
        }
    }
}

async fn run(cli: Cli) -> Result<i32> {
    let specs = select_tests(&cli)?;

    if cli.list {
        print_list();
        return Ok(0);
    }

    if specs.is_empty() {
        bail!("no tests selected");
    }

    let broker = parse_broker(&cli.broker)?;

    if cli.node_cmd.is_none() {
        let managed: Vec<_> = specs
            .iter()
            .filter(|spec| spec.nodes != NodeMode::Manual)
            .map(|spec| spec.id())
            .collect();
        if !managed.is_empty() {
            eprintln!(
                "warning: no --node-cmd given; the node under test must be started and stopped \
                 manually for: {}",
                managed.join(", ")
            );
        }
    }

    if let (true, None) = (
        specs.iter().any(|spec| spec.interactive),
        cli.console_reply.as_ref(),
    ) {
        eprintln!(
            "warning: interactive tests selected without --console-reply; prompts will go \
             unanswered and those tests will probably time out"
        );
    }

    let run_dir = new_run_dir(&cli.results_dir)?;
    eprintln!("Writing reports and logs to {}", run_dir.display());

    let mut container: Option<Container> = None;

    if let Some(image) = &cli.docker_image {
        if cli.docker_isolation == Isolation::Shared {
            let started = Container::start(image, &cli.docker_name).await?;
            Container::wait_ready(
                &broker.0,
                broker.1,
                Duration::from_secs(cli.startup_timeout),
            )
            .await?;
            eprintln!("TCK broker ready (container {})", started.name());
            container = Some(started);
        }
    }
    let mut results = Vec::with_capacity(specs.len());

    for (index, spec) in specs.iter().enumerate() {
        let last = index + 1 == specs.len();

        if let Some(image) = &cli.docker_image {
            if cli.docker_isolation == Isolation::PerTest {
                match start_ready_container(image, &cli.docker_name, &broker, &cli).await {
                    Ok(started) => container = Some(started),
                    Err(error) => {
                        let result = error_result(spec, &error);
                        report::print_result(&result);
                        results.push(result);
                        continue;
                    }
                }
            }
        }

        let result = match run_one(spec, &cli, &broker, &run_dir).await {
            Ok(result) => result,
            Err(error) => error_result(spec, &error),
        };

        report::print_result(&result);
        results.push(result);

        if cli.docker_isolation == Isolation::PerTest {
            if let Some(started) = container.take() {
                if cli.no_stop && last {
                    container = Some(started);
                } else {
                    started.stop().await;
                }
            }
        }
    }

    if let Some(started) = container.take() {
        if !cli.no_stop {
            started.stop().await;
        }
    }

    report::write_reports(
        &results,
        &run_dir,
        cli.junit.as_deref(),
        cli.json.as_deref(),
    )?;

    let failed = results
        .iter()
        .filter(|result| !result.outcome.is_pass())
        .count();
    println!();
    println!(
        "{} test(s): {} passed, {} failed",
        results.len(),
        results.len() - failed,
        failed
    );

    if failed > 0 {
        for result in results.iter().filter(|result| !result.outcome.is_pass()) {
            println!("  {:<45} {}", result.id(), result.outcome.as_str());
        }
        return Ok(1);
    }

    Ok(0)
}

async fn start_ready_container(
    image: &str,
    name: &str,
    broker: &(String, u16),
    cli: &Cli,
) -> Result<Container> {
    let started = Container::start(image, name).await?;
    if let Err(error) = Container::wait_ready(
        &broker.0,
        broker.1,
        Duration::from_secs(cli.startup_timeout),
    )
    .await
    {
        started.stop().await;
        return Err(error);
    }
    Ok(started)
}

async fn run_one(
    spec: &'static TestSpec,
    cli: &Cli,
    broker: &(String, u16),
    run_dir: &Path,
) -> Result<TestResult> {
    let started = Instant::now();
    let test_dir = run_dir.join(format!("{}-{}", spec.profile.as_str(), spec.name));
    std::fs::create_dir_all(&test_dir)
        .with_context(|| format!("creating {}", test_dir.display()))?;

    let params = TestParams {
        host_app_id: &cli.host_app_id,
        group_id: &cli.group_id,
        node_id: &cli.node_id,
        client_id: &cli.client_id,
        device_ids: &cli.device_ids,
        second_broker: &cli.second_broker,
        reorder_timeout: cli.reorder_timeout,
        broker_host: &broker.0,
        broker_port: broker.1,
    };

    let payload = spec.new_test_payload(&params)?;
    let mut log = String::new();

    let mut node = None;
    if matches!(spec.nodes, NodeMode::StartBefore) {
        node = start_node(cli, &test_dir)?;
        if node.is_some() {
            // Let the node connect and publish its retained birth or STATE
            // before the test starts observing it.
            sleep(Duration::from_millis(cli.node_start_delay)).await;
        }
    }

    let mut driver = Driver::connect(&broker.0, broker.1, Duration::from_secs(30))
        .await
        .context("connecting to the TCK broker")?;

    driver
        .publish_retained(
            driver::CONFIG_TOPIC,
            &format!("UTCwindow {}", cli.utc_window),
        )
        .await?;

    driver.publish(driver::TEST_CONTROL_TOPIC, &payload).await?;
    log.push_str(&format!("> {payload}\n"));

    if matches!(
        spec.nodes,
        NodeMode::StartAfter | NodeMode::StartAfterThenStop
    ) {
        sleep(Duration::from_millis(cli.node_start_delay)).await;
        node = start_node(cli, &test_dir)?;
    }

    let mut summary = None;
    let mut timed_out = false;
    let mut stop_at = match spec.nodes {
        NodeMode::StartAfterThenStop => {
            Some(Instant::now() + Duration::from_millis(cli.node_stop_delay))
        }
        _ => None,
    };
    let deadline = Instant::now() + Duration::from_secs(cli.timeout);

    loop {
        if let Some(at) = stop_at {
            if Instant::now() >= at {
                if let Some(node) = node.as_mut() {
                    eprintln!(
                        "[{}] stopping the node to trigger session termination",
                        spec.id()
                    );
                    node.stop(Duration::from_secs(10)).await;
                }
                stop_at = None;
            }
        }

        let now = Instant::now();
        if now >= deadline {
            timed_out = true;
            break;
        }

        let wait = match stop_at {
            Some(at) => at.saturating_duration_since(now).min(deadline - now),
            None => deadline - now,
        };

        match tokio::time::timeout(wait, driver.next()).await {
            Ok(Some(Message::Publish { topic, payload })) => {
                handle_message(
                    cli,
                    spec,
                    &mut driver,
                    &mut log,
                    &topic,
                    &payload,
                    &mut summary,
                )
                .await?;
                if summary.is_some() {
                    break;
                }
            }
            Ok(Some(Message::Error(error))) => bail!("MQTT error: {error}"),
            Ok(Some(Message::Connected)) => {}
            Ok(None) => bail!("MQTT event loop stopped"),
            Err(_) => {}
        }
    }

    if timed_out {
        log.push_str("[runner] timed out, sending END_TEST\n");
        eprintln!(
            "[{}] timed out after {}s, requesting the results so far",
            spec.id(),
            cli.timeout
        );
        let _ = driver.publish(driver::TEST_CONTROL_TOPIC, "END_TEST").await;

        let end = Instant::now() + Duration::from_secs(10);
        while Instant::now() < end && summary.is_none() {
            let remaining = end.saturating_duration_since(Instant::now());
            match tokio::time::timeout(remaining, driver.next()).await {
                Ok(Some(Message::Publish { topic, payload })) => {
                    handle_message(
                        cli,
                        spec,
                        &mut driver,
                        &mut log,
                        &topic,
                        &payload,
                        &mut summary,
                    )
                    .await?;
                }
                Ok(Some(Message::Error(error))) => bail!("MQTT error: {error}"),
                Ok(Some(Message::Connected)) => {}
                Ok(None) => break,
                Err(_) => break,
            }
        }
    }

    if let Some(mut node) = node.take() {
        node.stop(Duration::from_secs(10)).await;
    }

    std::fs::write(test_dir.join("tck.log"), &log)
        .with_context(|| format!("writing {}", test_dir.join("tck.log").display()))?;

    let (outcome, message) = if timed_out {
        let partial = summary
            .as_ref()
            .map(|summary| format!(" (partial result: {})", summary.overall_detail))
            .unwrap_or_default();
        (
            Outcome::TimedOut,
            Some(format!("no result within {}s{partial}", cli.timeout)),
        )
    } else {
        match &summary {
            Some(summary) => match summary.overall {
                Status::Pass => (Outcome::Passed, None),
                Status::NotExecuted => (Outcome::NotExecuted, Some(summary.overall_detail.clone())),
                _ => (Outcome::Failed, Some(summary.overall_detail.clone())),
            },
            None => (
                Outcome::Error,
                Some("the test ended without publishing a result".to_string()),
            ),
        }
    };

    Ok(TestResult {
        profile: spec.profile.as_str().to_string(),
        name: spec.name.to_string(),
        command: payload,
        outcome,
        summary,
        message,
        duration_secs: started.elapsed().as_secs_f64(),
        log,
    })
}

async fn handle_message(
    cli: &Cli,
    spec: &TestSpec,
    driver: &mut Driver,
    log: &mut String,
    topic: &str,
    payload: &str,
    summary: &mut Option<report::Summary>,
) -> Result<()> {
    match topic {
        driver::RESULT_TOPIC => {
            log.push_str(&format!("[RESULT] {payload}\n"));
            if let Some(parsed) = report::parse_summary(payload) {
                *summary = Some(parsed);
            }
        }
        driver::LOG_TOPIC => {
            log.push_str(&format!("[LOG] {payload}\n"));
            if cli.verbose {
                println!("[{}] {payload}", spec.id());
            }
        }
        driver::CONSOLE_PROMPT_TOPIC => {
            log.push_str(&format!("[PROMPT] {payload}\n"));
            match &cli.console_reply {
                Some(reply) => {
                    log.push_str(&format!("[runner] replying with `{reply}`\n"));
                    driver.publish(driver::CONSOLE_REPLY_TOPIC, reply).await?;
                }
                None => log
                    .push_str("[runner] no --console-reply set, the test may wait for an answer\n"),
            }
        }
        _ => {}
    }
    Ok(())
}

fn start_node(cli: &Cli, test_dir: &Path) -> Result<Option<NodeProcess>> {
    let Some(command) = &cli.node_cmd else {
        return Ok(None);
    };

    let log_path = test_dir.join("node.log");
    let node = NodeProcess::spawn(command, &log_path)?;
    eprintln!(
        "[runner] started node: {command} (log: {})",
        log_path.display()
    );
    Ok(Some(node))
}

fn error_result(spec: &TestSpec, error: &anyhow::Error) -> TestResult {
    TestResult {
        profile: spec.profile.as_str().to_string(),
        name: spec.name.to_string(),
        command: String::new(),
        outcome: Outcome::Error,
        summary: None,
        message: Some(format!("{error:#}")),
        duration_secs: 0.0,
        log: String::new(),
    }
}

fn select_tests(cli: &Cli) -> Result<Vec<&'static TestSpec>> {
    if !cli.tests.is_empty() {
        return cli
            .tests
            .iter()
            .map(|id| catalog::find(id))
            .collect::<Result<Vec<_>>>();
    }

    Ok(match cli.suite {
        Suite::Edge => catalog::by_profile(Profile::Edge).collect(),
        Suite::Host => catalog::by_profile(Profile::Host).collect(),
        Suite::Broker => catalog::by_profile(Profile::Broker).collect(),
        Suite::All => catalog::TESTS.iter().collect(),
    })
}

fn print_list() {
    for profile in [Profile::Edge, Profile::Host, Profile::Broker] {
        println!("{}", profile.as_str().to_uppercase());
        for spec in catalog::by_profile(profile) {
            println!("  {:<45} {}", spec.id(), spec.description);
        }
        println!();
    }
}

fn parse_broker(value: &str) -> Result<(String, u16)> {
    let value = value
        .strip_prefix("mqtt://")
        .or_else(|| value.strip_prefix("tcp://"))
        .unwrap_or(value);

    match value.rsplit_once(':') {
        Some((host, port)) => Ok((
            host.trim_matches(['[', ']']).to_string(),
            port.parse()
                .with_context(|| format!("invalid broker port '{port}'"))?,
        )),
        None => Ok((value.to_string(), 1883)),
    }
}

fn new_run_dir(results_dir: &Path) -> Result<PathBuf> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs();

    let mut run_dir = results_dir.join(timestamp.to_string());
    let mut suffix = 1;
    while run_dir.exists() {
        run_dir = results_dir.join(format!("{timestamp}-{suffix}"));
        suffix += 1;
    }

    std::fs::create_dir_all(&run_dir).with_context(|| format!("creating {}", run_dir.display()))?;
    Ok(run_dir)
}
