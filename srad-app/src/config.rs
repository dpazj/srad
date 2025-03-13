use srad_types::topic::{QoS, Topic, TopicFilter};



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

#[derive(Clone)]
pub enum SubscriptionConfig{
    AllGroups,
    SingleGroup{group_id: String},
    Custom(Vec<NamespaceSubConfig>)
}

impl From<SubscriptionConfig> for Vec<TopicFilter> {
    fn from(value: SubscriptionConfig) -> Self {
        match value {
            SubscriptionConfig::AllGroups => vec![TopicFilter { topic: Topic::Namespace, qos: QoS::AtMostOnce }],
            SubscriptionConfig::SingleGroup { group_id } => vec![
                TopicFilter { topic: Topic::Group { id: group_id }, qos: QoS::AtMostOnce}
            ],
            SubscriptionConfig::Custom(namespace_sub_configs) => {
                namespace_sub_configs.into_iter().map(TopicFilter::from).collect()
            },
        }
    }
}
