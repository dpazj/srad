//! Parsing of TCK results and generation of reports.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Status {
    Pass,
    Fail,
    NotExecuted,
    Maybe,
    NotImplemented,
    Unknown,
}

impl Status {
    fn parse(value: &str) -> Self {
        let value = value.trim();
        if value.starts_with("PASS") {
            Status::Pass
        } else if value.starts_with("FAIL") {
            Status::Fail
        } else if value.starts_with("NOT EXECUTED") {
            Status::NotExecuted
        } else if value.starts_with("MAYBE") {
            Status::Maybe
        } else if value.starts_with("NOT YET IMPLEMENTED") {
            Status::NotImplemented
        } else {
            Status::Unknown
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Assertion {
    pub key: String,
    pub status: Status,
    /// Additional text reported by the TCK, e.g. the failing requirement.
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub title: Option<String>,
    pub overall: Status,
    /// The raw overall value, e.g. `FAIL but INCOMPLETE`.
    pub overall_detail: String,
    pub assertions: Vec<Assertion>,
}

impl Summary {
    pub fn counts(&self) -> BTreeMap<Status, usize> {
        let mut counts = BTreeMap::new();
        for assertion in &self.assertions {
            *counts.entry(assertion.status).or_default() += 1;
        }
        counts
    }

    pub fn failures(&self) -> impl Iterator<Item = &Assertion> {
        self.assertions
            .iter()
            .filter(|assertion| assertion.status == Status::Fail)
    }
}

/// Parse a `SPARKPLUG_TCK/RESULT` message. Returns `None` for messages that
/// are not a complete test summary (the TCK also publishes progress messages
/// on this topic).
pub fn parse_summary(payload: &str) -> Option<Summary> {
    let mut title = None;
    let mut assertions = Vec::new();
    let mut overall = None;

    for line in payload.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(index) = line.find(" Summary Test Results for ") {
            title = Some(line[index + " Summary Test Results for ".len()..].to_string());
            continue;
        }

        let line = line.strip_suffix(';').unwrap_or(line);
        let Some((key, value)) = line.split_once(": ") else {
            continue;
        };

        if key == "OVERALL" {
            overall = Some((Status::parse(value), value.trim().to_string()));
        } else {
            assertions.push(Assertion {
                key: key.to_string(),
                status: Status::parse(value),
                detail: value.trim().to_string(),
            });
        }
    }

    let (overall, overall_detail) = overall?;
    Some(Summary {
        title,
        overall,
        overall_detail,
        assertions,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Passed,
    Failed,
    NotExecuted,
    TimedOut,
    Error,
}

impl Outcome {
    pub fn is_pass(self) -> bool {
        self == Outcome::Passed
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Passed => "PASS",
            Outcome::Failed => "FAIL",
            Outcome::NotExecuted => "NOT EXECUTED",
            Outcome::TimedOut => "TIMEOUT",
            Outcome::Error => "ERROR",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TestResult {
    pub profile: String,
    pub name: String,
    pub command: String,
    pub outcome: Outcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<Summary>,
    pub message: Option<String>,
    pub duration_secs: f64,
    pub log: String,
}

impl TestResult {
    pub fn id(&self) -> String {
        format!("{}:{}", self.profile, self.name)
    }
}

pub fn print_result(result: &TestResult) {
    println!(
        "{:<45} {:<13} {:6.1}s",
        result.id(),
        result.outcome.as_str(),
        result.duration_secs
    );

    if let Some(message) = &result.message {
        println!("    {message}");
    }

    if let Some(summary) = &result.summary {
        let counts = summary.counts();
        let mut parts = Vec::new();
        for (status, count) in counts {
            parts.push(format!("{} {}", count, status_name(status)));
        }
        println!(
            "    {} assertions: {}",
            summary.assertions.len(),
            parts.join(", ")
        );

        for failure in summary.failures() {
            println!("    FAIL {}: {}", failure.key, failure.detail);
        }
    }
}

fn status_name(status: Status) -> &'static str {
    match status {
        Status::Pass => "passed",
        Status::Fail => "failed",
        Status::NotExecuted => "not executed",
        Status::Maybe => "maybe",
        Status::NotImplemented => "not implemented",
        Status::Unknown => "unknown",
    }
}

pub fn write_reports(
    results: &[TestResult],
    output_dir: &Path,
    junit: Option<&Path>,
    json: Option<&Path>,
) -> Result<()> {
    fs::create_dir_all(output_dir).with_context(|| format!("creating {}", output_dir.display()))?;

    let json_path = json
        .map(Path::to_path_buf)
        .unwrap_or_else(|| output_dir.join("results.json"));
    let junit_path = junit
        .map(Path::to_path_buf)
        .unwrap_or_else(|| output_dir.join("junit.xml"));

    fs::write(&json_path, to_json(results))
        .with_context(|| format!("writing {}", json_path.display()))?;
    fs::write(&junit_path, to_junit(results))
        .with_context(|| format!("writing {}", junit_path.display()))?;

    println!();
    println!("JSON report:  {}", json_path.display());
    println!("JUnit report: {}", junit_path.display());
    Ok(())
}

fn to_json(results: &[TestResult]) -> String {
    #[derive(Serialize)]
    struct Report<'a> {
        generated_at_unix: u64,
        results: &'a [TestResult],
    }

    let report = Report {
        generated_at_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs(),
        results,
    };

    serde_json::to_string_pretty(&report)
        .unwrap_or_else(|error| format!(r#"{{"error":"failed to serialise report: {error}"}}"#))
}

fn to_junit(results: &[TestResult]) -> String {
    let mut output = String::new();
    let failures = results
        .iter()
        .filter(|result| !result.outcome.is_pass())
        .count();
    let total_time: f64 = results.iter().map(|result| result.duration_secs).sum();

    output.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        output,
        "<testsuites name=\"sparkplug-tck\" tests=\"{}\" failures=\"{}\" time=\"{total_time:.3}\">",
        results.len(),
        failures
    );

    let mut profiles: Vec<&str> = results
        .iter()
        .map(|result| result.profile.as_str())
        .collect();
    profiles.sort_unstable();
    profiles.dedup();

    for profile in profiles {
        let cases: Vec<_> = results
            .iter()
            .filter(|result| result.profile == profile)
            .collect();
        let profile_failures = cases
            .iter()
            .filter(|result| !result.outcome.is_pass())
            .count();
        let profile_time: f64 = cases.iter().map(|result| result.duration_secs).sum();

        let _ = writeln!(
            output,
            "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{profile_failures}\" time=\"{profile_time:.3}\">",
            xml_escape(profile),
            cases.len()
        );

        for case in cases {
            let _ = writeln!(
                output,
                "    <testcase classname=\"{}\" name=\"{}\" time=\"{:.3}\">",
                xml_escape(&case.profile),
                xml_escape(&case.name),
                case.duration_secs
            );

            if !case.outcome.is_pass() {
                let message = case
                    .message
                    .as_deref()
                    .map(str::to_string)
                    .or_else(|| {
                        case.summary
                            .as_ref()
                            .map(|summary| summary.overall_detail.clone())
                    })
                    .unwrap_or_else(|| case.outcome.as_str().to_string());

                let mut details = String::new();
                if let Some(summary) = &case.summary {
                    for failure in summary.failures() {
                        let _ = writeln!(details, "{}: {}", failure.key, failure.detail);
                    }
                }

                let _ = writeln!(
                    output,
                    "      <failure message=\"{}\">{}</failure>",
                    xml_escape(&message),
                    xml_escape(&details)
                );
            }

            let _ = writeln!(
                output,
                "      <system-out>{}</system-out>",
                xml_escape(&case.log)
            );
            output.push_str("    </testcase>\n");
        }

        output.push_str("  </testsuite>\n");
    }

    output.push_str("</testsuites>\n");
    output
}

fn xml_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            // Strip characters that are not allowed in XML 1.0 documents.
            character
                if (character as u32) < 0x20
                    && character != '\n'
                    && character != '\t'
                    && character != '\r' => {}
            character => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAYLOAD: &str = "\
2026-11-03 17:38:08.371 Summary Test Results for Edge SessionEstablishment
Monitor:topic-structure: PASS;
message-flow-edge-node-birth-publish-phid-wait: FAIL [tck-id-message-flow-edge-node-birth-publish-phid-wait] Some requirement;
principles-persistence-clean-session-311: NOT EXECUTED;
OVERALL: FAIL but INCOMPLETE;
";

    #[test]
    fn parses_a_summary() {
        let summary = parse_summary(PAYLOAD).unwrap();
        assert_eq!(summary.title.as_deref(), Some("Edge SessionEstablishment"));
        assert_eq!(summary.overall, Status::Fail);
        assert_eq!(summary.overall_detail, "FAIL but INCOMPLETE");
        assert_eq!(summary.assertions.len(), 3);
        assert_eq!(summary.assertions[0].key, "Monitor:topic-structure");
        assert_eq!(summary.assertions[1].status, Status::Fail);
        assert_eq!(summary.assertions[2].status, Status::NotExecuted);
    }

    #[test]
    fn ignores_progress_messages() {
        assert!(parse_summary("starting test").is_none());
    }

    #[test]
    fn escapes_xml() {
        assert_eq!(xml_escape("a<b>&\"'"), "a&lt;b&gt;&amp;&quot;&apos;");
    }
}
