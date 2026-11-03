//! Catalogue of the tests exposed by the Eclipse Sparkplug TCK 3.0.0.
//!
//! The TCK is driven entirely over MQTT: a test is started by publishing
//! `NEW_TEST <profile> <test> <arguments...>` to `SPARKPLUG_TCK/TEST_CONTROL`
//! and its result is published to `SPARKPLUG_TCK/RESULT` when it finishes.
//! This module records which arguments each test expects and how the node
//! under test has to be managed while the test runs.

use anyhow::{bail, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Host,
    Edge,
    Broker,
}

impl Profile {
    /// Name used by the TCK in the `NEW_TEST` command.
    pub fn as_tck(self) -> &'static str {
        match self {
            Profile::Host => "host",
            Profile::Edge => "edge",
            Profile::Broker => "broker",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Profile::Host => "host",
            Profile::Edge => "edge",
            Profile::Broker => "broker",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "host" | "hostapplication" => Some(Profile::Host),
            "edge" | "eon" | "eonnode" => Some(Profile::Edge),
            "broker" => Some(Profile::Broker),
            _ => None,
        }
    }
}

/// How the node under test has to be managed while a test runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeMode {
    /// Start the node after the test has been requested. Needed by tests that
    /// observe the initial `CONNECT` and the birth sequence.
    StartAfter,
    /// Start the node before the test has been requested.
    StartBefore,
    /// Start the node after the test has been requested, then stop it while
    /// the test runs. Used by session termination tests, which observe the
    /// disconnect.
    StartAfterThenStop,
    /// Leave node management to the user.
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceArgs {
    /// The test does not take a device id.
    None,
    /// The test takes a single device id.
    One,
    /// The test takes every configured device id as a separate argument.
    All,
}

#[derive(Debug, Clone, Copy)]
pub struct TestSpec {
    pub profile: Profile,
    pub name: &'static str,
    pub nodes: NodeMode,
    pub devices: DeviceArgs,
    /// The test asks the operator questions over `SPARKPLUG_TCK/CONSOLE_PROMPT`.
    pub interactive: bool,
    pub description: &'static str,
}

const fn test(
    profile: Profile,
    name: &'static str,
    nodes: NodeMode,
    devices: DeviceArgs,
    interactive: bool,
    description: &'static str,
) -> TestSpec {
    TestSpec {
        profile,
        name,
        nodes,
        devices,
        interactive,
        description,
    }
}

/// Every test shipped with the TCK 3.0.0.
pub const TESTS: &[TestSpec] = &[
    // Edge Node profile
    test(
        Profile::Edge,
        "SessionEstablishmentTest",
        NodeMode::StartAfter,
        DeviceArgs::All,
        false,
        "Edge node session establishment and birth sequence",
    ),
    test(
        Profile::Edge,
        "SessionTerminationTest",
        NodeMode::StartAfterThenStop,
        DeviceArgs::One,
        false,
        "Edge node intentional disconnect and death messages",
    ),
    test(
        Profile::Edge,
        "SendDataTest",
        NodeMode::StartAfter,
        DeviceArgs::One,
        false,
        "Edge node and device data messages",
    ),
    test(
        Profile::Edge,
        "SendComplexDataTest",
        NodeMode::StartAfter,
        DeviceArgs::One,
        false,
        "Edge node and device data messages with complex types",
    ),
    test(
        Profile::Edge,
        "ReceiveCommandTest",
        NodeMode::StartAfter,
        DeviceArgs::One,
        false,
        "Edge node and device command handling",
    ),
    test(
        Profile::Edge,
        "PrimaryHostTest",
        NodeMode::StartBefore,
        DeviceArgs::One,
        false,
        "Edge node re-birth when the primary host application changes",
    ),
    test(
        Profile::Edge,
        "MultipleBrokerTest",
        NodeMode::StartAfter,
        DeviceArgs::One,
        false,
        "Edge node behaviour with multiple brokers",
    ),
    // Host Application profile
    test(
        Profile::Host,
        "SessionEstablishmentTest",
        NodeMode::Manual,
        DeviceArgs::None,
        false,
        "Host application session establishment",
    ),
    test(
        Profile::Host,
        "SessionTerminationTest",
        NodeMode::Manual,
        DeviceArgs::None,
        false,
        "Host application intentional disconnect",
    ),
    test(
        Profile::Host,
        "SendCommandTest",
        NodeMode::Manual,
        DeviceArgs::One,
        true,
        "Host application sends node and device commands",
    ),
    test(
        Profile::Host,
        "EdgeSessionTerminationTest",
        NodeMode::Manual,
        DeviceArgs::One,
        true,
        "Host application handles edge node death messages",
    ),
    test(
        Profile::Host,
        "MessageOrderingTest",
        NodeMode::Manual,
        DeviceArgs::One,
        false,
        "Host application reorders out of order messages",
    ),
    test(
        Profile::Host,
        "MultipleBrokerTest",
        NodeMode::Manual,
        DeviceArgs::None,
        false,
        "Host application behaviour with multiple brokers",
    ),
    // Broker profile
    test(
        Profile::Broker,
        "CompliantBrokerTest",
        NodeMode::Manual,
        DeviceArgs::None,
        false,
        "Broker Sparkplug compliance",
    ),
    test(
        Profile::Broker,
        "AwareBrokerTest",
        NodeMode::Manual,
        DeviceArgs::None,
        false,
        "Broker Sparkplug awareness",
    ),
];

/// Parameters shared by all tests in a run.
pub struct TestParams<'a> {
    pub host_app_id: &'a str,
    pub group_id: &'a str,
    pub node_id: &'a str,
    pub client_id: &'a str,
    pub device_ids: &'a [String],
    pub second_broker: &'a str,
    pub reorder_timeout: u64,
    pub broker_host: &'a str,
    pub broker_port: u16,
}

impl TestSpec {
    /// The `profile:name` identifier used on the command line.
    pub fn id(&self) -> String {
        format!("{}:{}", self.profile.as_str(), self.name)
    }

    /// Arguments appended to the `NEW_TEST` command for this test.
    pub fn args(&self, params: &TestParams<'_>) -> Result<Vec<String>> {
        let mut args = Vec::new();

        match (self.profile, self.name) {
            (Profile::Broker, "CompliantBrokerTest") => {
                args.push(params.broker_host.to_string());
                args.push(params.broker_port.to_string());
            }
            (Profile::Broker, "AwareBrokerTest") => {
                args.push(params.broker_host.to_string());
                args.push(params.broker_port.to_string());
                args.push(params.group_id.to_string());
                args.push(params.node_id.to_string());
            }
            (Profile::Host, "SessionEstablishmentTest") => {
                args.push(params.host_app_id.to_string());
            }
            (Profile::Host, "SessionTerminationTest") => {
                args.push(params.host_app_id.to_string());
                args.push(params.client_id.to_string());
            }
            (Profile::Host, "MultipleBrokerTest") => {
                args.push(params.host_app_id.to_string());
                args.push(params.second_broker.to_string());
            }
            (Profile::Host, "SendCommandTest" | "EdgeSessionTerminationTest") => {
                args.push(params.host_app_id.to_string());
                args.push(params.group_id.to_string());
                args.push(params.node_id.to_string());
                args.push(self.device_arg(params)?);
            }
            (Profile::Host, "MessageOrderingTest") => {
                args.push(params.host_app_id.to_string());
                args.push(params.group_id.to_string());
                args.push(params.node_id.to_string());
                args.push(self.device_arg(params)?);
                args.push(params.reorder_timeout.to_string());
            }
            (Profile::Edge, "SessionEstablishmentTest") => {
                args.push(params.host_app_id.to_string());
                args.push(params.group_id.to_string());
                args.push(params.node_id.to_string());
                args.extend(params.device_ids.iter().cloned());
            }
            (Profile::Edge, "MultipleBrokerTest") => {
                args.push(params.host_app_id.to_string());
                args.push(params.group_id.to_string());
                args.push(params.node_id.to_string());
                args.push(self.device_arg(params)?);
                args.push(params.second_broker.to_string());
            }
            (Profile::Edge, _) => {
                args.push(params.host_app_id.to_string());
                args.push(params.group_id.to_string());
                args.push(params.node_id.to_string());
                args.push(self.device_arg(params)?);
            }
            (_, name) => bail!("no argument template for {name}"),
        }

        Ok(args)
    }

    fn device_arg(&self, params: &TestParams<'_>) -> Result<String> {
        match self.devices {
            DeviceArgs::None => Ok(String::new()),
            DeviceArgs::One | DeviceArgs::All => match params.device_ids.first() {
                Some(id) if !id.trim().is_empty() => Ok(id.clone()),
                _ => bail!("{} requires a device id, pass --device-id <ID>", self.id()),
            },
        }
    }

    /// The full `NEW_TEST` payload.
    pub fn new_test_payload(&self, params: &TestParams<'_>) -> Result<String> {
        let mut payload = format!("NEW_TEST {} {}", self.profile.as_tck(), self.name);
        for arg in self.args(params)? {
            if !arg.is_empty() {
                payload.push(' ');
                payload.push_str(&arg);
            }
        }
        Ok(payload)
    }
}

/// Look up a test by its `profile:name` identifier.
pub fn find(id: &str) -> Result<&'static TestSpec> {
    let (profile, name) = match id.split_once(':') {
        Some((profile, name)) => {
            let profile = Profile::parse(profile).ok_or_else(|| {
                anyhow::anyhow!(
                    "unknown profile '{profile}' in '{id}' (expected edge, host or broker)"
                )
            })?;
            (Some(profile), name)
        }
        None => (None, id),
    };

    let matches: Vec<_> = TESTS
        .iter()
        .filter(|spec| spec.name.eq_ignore_ascii_case(name))
        .filter(|spec| profile.is_none_or(|profile| spec.profile == profile))
        .collect();

    match matches.as_slice() {
        [spec] => Ok(spec),
        [] => bail!("unknown test '{id}'. Run `tck-runner --list` to see the available tests"),
        _ => bail!(
            "test '{name}' exists in several profiles, use <profile>:<test>, e.g. {}",
            matches[0].id()
        ),
    }
}

/// All tests belonging to a profile.
pub fn by_profile(profile: Profile) -> impl Iterator<Item = &'static TestSpec> {
    TESTS.iter().filter(move |spec| spec.profile == profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> TestParams<'static> {
        TestParams {
            host_app_id: "host",
            group_id: "group",
            node_id: "node",
            client_id: "client",
            device_ids: &[],
            second_broker: "tcp://localhost:1884",
            reorder_timeout: 5000,
            broker_host: "localhost",
            broker_port: 1883,
        }
    }

    #[test]
    fn all_ids_are_unique() {
        let mut ids: Vec<_> = TESTS.iter().map(TestSpec::id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), TESTS.len());
    }

    #[test]
    fn parses_profiles() {
        assert_eq!(find("edge:SendDataTest").unwrap().profile, Profile::Edge);
        assert_eq!(find("host:SendCommandTest").unwrap().profile, Profile::Host);
        assert_eq!(
            find("broker:CompliantBrokerTest").unwrap().profile,
            Profile::Broker
        );
        assert!(find("nope:SendDataTest").is_err());
        assert!(find("SessionEstablishmentTest").is_err());
    }

    #[test]
    fn builds_edge_payload() {
        let spec = find("edge:SessionEstablishmentTest").unwrap();
        let mut params = params();
        let devices = vec!["dev1".to_string(), "dev2".to_string()];
        params.device_ids = &devices;
        assert_eq!(
            spec.new_test_payload(&params).unwrap(),
            "NEW_TEST edge SessionEstablishmentTest host group node dev1 dev2"
        );
    }

    #[test]
    fn edge_device_test_needs_a_device() {
        let spec = find("edge:SendDataTest").unwrap();
        let params = params();
        assert!(spec.new_test_payload(&params).is_err());
    }
}
