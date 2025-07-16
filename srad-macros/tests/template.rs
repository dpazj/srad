use srad::types::{
    Template, TemplateDefinition, TemplateInstance, TemplateMetadata,
    TemplateMetric, TemplateParameter,
};

#[derive(Template, Clone, Default)]
struct Simple {
    x: i32,
    y: i32,
    z: i32,
}

impl TemplateMetadata for Simple {
    fn template_name() -> &'static str {
        "test"
    }
}

#[test]
pub fn test_simple() {
    let definition = Simple::template_definition();
    assert_eq!(
        definition,
        TemplateDefinition {
            version: None,
            metrics: vec![
                TemplateMetric::new_template_metric("x".into(), 0),
                TemplateMetric::new_template_metric("y".into(), 0),
                TemplateMetric::new_template_metric("z".into(), 0)
            ],
            parameters: vec![],
        }
    );

    let simple = Simple::default();
    let instance = simple.template_instance();
    assert_eq!(
        instance,
        TemplateInstance {
            version: None,
            metrics: vec![
                TemplateMetric::new_template_metric("x".into(), 0),
                TemplateMetric::new_template_metric("y".into(), 0),
                TemplateMetric::new_template_metric("z".into(), 0)
            ],
            parameters: vec![],
            template_ref: Simple::template_definition_metric_name()
        }
    );
}

#[derive(Template)]
struct NestedTemplate {
    first: i32,
    nested: Simple,
}

impl TemplateMetadata for NestedTemplate {
    fn template_name() -> &'static str {
        "nested"
    }
}

#[test]
pub fn test_nested() {
    let definition = NestedTemplate::template_definition();
    assert_eq!(
        definition,
        TemplateDefinition {
            version: None,
            metrics: vec![
                TemplateMetric::new_template_metric("first".into(), 0),
                TemplateMetric::new_template_metric(
                    "nested".into(),
                    TemplateInstance {
                        version: None,
                        metrics: vec![
                            TemplateMetric::new_template_metric("x".into(), 0),
                            TemplateMetric::new_template_metric("y".into(), 0),
                            TemplateMetric::new_template_metric("z".into(), 0)
                        ],
                        parameters: vec![],
                        template_ref: Simple::template_definition_metric_name()
                    }
                ),
            ],
            parameters: vec![],
        }
    );

    let nested = NestedTemplate {
        first: 1,
        nested: Simple { x: 1, y: 2, z: 3 },
    };

    assert_eq!(
        nested.template_instance(),
        TemplateInstance {
            version: None,
            metrics: vec![
                TemplateMetric::new_template_metric("first".into(), 1),
                TemplateMetric::new_template_metric(
                    "nested".into(),
                    TemplateInstance {
                        version: None,
                        metrics: vec![
                            TemplateMetric::new_template_metric("x".into(), 1),
                            TemplateMetric::new_template_metric("y".into(), 2),
                            TemplateMetric::new_template_metric("z".into(), 3)
                        ],
                        parameters: vec![],
                        template_ref: Simple::template_definition_metric_name()
                    }
                )
            ],
            parameters: vec![],
            template_ref: NestedTemplate::template_definition_metric_name()
        }
    );
}

#[test]
pub fn test_attribute_default() {
    #[derive(Template)]
    struct TemplateDefault {
        #[template(default = true)]
        default_bool: bool,
        #[template(default = 42)]
        default_int: i32,
        #[template(default="Hello, World!".into())]
        default_string: String,
    }

    impl TemplateMetadata for TemplateDefault {
        fn template_name() -> &'static str {
            "default"
        }
    }

    let definition = TemplateDefault::template_definition();
    assert_eq!(
        definition,
        TemplateDefinition {
            version: None,
            metrics: vec![
                TemplateMetric::new_template_metric("default_bool".into(), true),
                TemplateMetric::new_template_metric("default_int".into(), 42_i32),
                TemplateMetric::new_template_metric(
                    "default_string".into(),
                    "Hello, World!".to_string()
                ),
            ],
            parameters: vec![],
        }
    );
}

#[test]
pub fn test_attribute_skip() {
    #[derive(Template)]
    struct TemplateSkip {
        not_skipped: u32,
        #[allow(dead_code)]
        #[template(skip)]
        skipped: i32,
    }

    impl TemplateMetadata for TemplateSkip {
        fn template_name() -> &'static str {
            "skip"
        }
    }

    let definition = TemplateSkip::template_definition();
    assert_eq!(
        definition,
        TemplateDefinition {
            version: None,
            metrics: vec![TemplateMetric::new_template_metric(
                "not_skipped".into(),
                0_u32
            ),],
            parameters: vec![],
        }
    );

    let skip = TemplateSkip {
        not_skipped: 42,
        skipped: 123,
    };
    let instance = skip.template_instance();
    assert_eq!(
        instance,
        TemplateInstance {
            version: None,
            metrics: vec![TemplateMetric::new_template_metric(
                "not_skipped".into(),
                42_u32
            ),],
            parameters: vec![],
            template_ref: TemplateSkip::template_definition_metric_name()
        }
    );
}

#[test]
pub fn test_attribute_rename() {
    #[derive(Template)]
    struct TemplateRename {
        not_renamed: u32,
        #[template(rename = "custom_name")]
        renamed_field: i32,
    }

    impl TemplateMetadata for TemplateRename {
        fn template_name() -> &'static str {
            "rename"
        }
    }

    let definition = TemplateRename::template_definition();
    assert_eq!(
        definition,
        TemplateDefinition {
            version: None,
            metrics: vec![
                TemplateMetric::new_template_metric("not_renamed".into(), 0_u32),
                TemplateMetric::new_template_metric("custom_name".into(), 0_i32),
            ],
            parameters: vec![],
        }
    );
}

#[test]
pub fn test_parameter() {
    #[derive(Template)]
    struct TemplateWithParameter {
        metric_field: u32,
        #[template(parameter)]
        parameter_field: u32,
    }

    impl TemplateMetadata for TemplateWithParameter {
        fn template_name() -> &'static str {
            "parameter"
        }
    }
    let definition = TemplateWithParameter::template_definition();
    assert_eq!(
        definition,
        TemplateDefinition {
            version: None,
            metrics: vec![TemplateMetric::new_template_metric(
                "metric_field".into(),
                0_u32
            ),],
            parameters: vec![TemplateParameter::new_template_parameter(
                "parameter_field".into(),
                0_u32
            ),],
        }
    );
}
