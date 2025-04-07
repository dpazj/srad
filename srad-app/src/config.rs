use srad_types::topic::{QoS, Topic, TopicFilter};

/// Subscription configuration to allow for fine-grain control of what group or node to receive messages from 
#[derive(Clone)]
pub enum NamespaceSubConfig {
    Group {group_id: String},
    Node {group_id: String, node_id: String},
}

impl From<NamespaceSubConfig> for TopicFilter {
    fn from(value: NamespaceSubConfig) -> Self {
        match value {
            NamespaceSubConfig::Group { group_id } => {
                TopicFilter { topic: Topic::Group { id: group_id }, qos: QoS::AtMostOnce}
            },
            NamespaceSubConfig::Node { group_id, node_id } => {
                TopicFilter { topic: Topic::Node { group_id, node_id }, qos: QoS::AtMostOnce}
            },
        }
    }
}

/// Subscription configuration for the topics the application will subscribe to for node and device messages
#[derive(Clone)]
pub enum SubscriptionConfig{
    /// Subscribe to all groups messages
    AllGroups,
    /// Subscribe to a single group's messages
    SingleGroup{group_id: String},
    /// Custom subscription configuration
    Custom(Vec<NamespaceSubConfig>)
}

impl From<SubscriptionConfig> for Vec<TopicFilter> {
    fn from(value: SubscriptionConfig) -> Self {
        match value {
            SubscriptionConfig::AllGroups => vec![
                TopicFilter { topic: Topic::FullNamespace, qos: QoS::AtMostOnce }
            ],
            SubscriptionConfig::SingleGroup { group_id } => vec![
                TopicFilter { topic: Topic::Group { id: group_id }, qos: QoS::AtMostOnce}
            ],
            SubscriptionConfig::Custom(namespace_sub_configs) => {
                namespace_sub_configs.into_iter().map(TopicFilter::from).collect()
            },
        }
    }
}
