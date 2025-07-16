use srad::types::{
    Template, TemplateDefinition, TemplateError, TemplateInstance, TemplateMetadata, TemplateMetric, TemplateParameter
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
    fn template_version() -> Option<&'static str> {
        Some("1.0")
    }
}

#[test]
pub fn test_simple() {
    let definition = Simple::template_definition();
    assert_eq!(
        definition,
        TemplateDefinition {
            version: Some("1.0".into()),
            metrics: vec![
                TemplateMetric::new_template_metric("x".into(), 0),
                TemplateMetric::new_template_metric("y".into(), 0),
                TemplateMetric::new_template_metric("z".into(), 0)
            ],
            parameters: vec![],
        }
    );

    let simple = Simple::default();
    let valid_instance = || {
        TemplateInstance {
            version: Some("1.0".into()),
            metrics: vec![
                TemplateMetric::new_template_metric("x".into(), 0),
                TemplateMetric::new_template_metric("y".into(), 0),
                TemplateMetric::new_template_metric("z".into(), 0)
            ],
            parameters: vec![],
            template_ref: Simple::template_definition_metric_name()
        }
    };
    assert_eq!(
        simple.template_instance(),
        valid_instance()
    );

    assert!(
        Simple::try_from(
            valid_instance()
        ).is_ok()
    );

    let mut invalid_version = valid_instance();
    invalid_version.version = None;
    assert!(matches!(
        Simple::try_from(
            invalid_version
        ),
        Err(TemplateError::VersionMismatch)
    ));

    let mut invalid_ref= valid_instance();
    invalid_ref.template_ref = "INVALID".into();
    assert!(matches!(
        Simple::try_from(
            invalid_ref
        ),
        Err(TemplateError::RefMismatch(_))
    ));

    let mut unknown_metric= valid_instance();
    unknown_metric.metrics.push(TemplateMetric::new_template_metric("UNKNOWN".into(), 0));
    assert!(matches!(
        Simple::try_from(
            unknown_metric 
        ),
        Err(TemplateError::UnknownMetric(_))
    ));

    let mut unknown_param= valid_instance();
    unknown_param.parameters.push(TemplateParameter::new_template_parameter("UNKNOWN".into(), 0));
    assert!(matches!(
        Simple::try_from(
            unknown_param
        ),
        Err(TemplateError::UnknownParameter(_))
    ));

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
                TemplateMetric::new_template_metric("first".into(), 0_i32),
                TemplateMetric::new_template_metric(
                    "nested".into(),
                    TemplateInstance {
                        version: Simple::template_version().map(String::from),
                        metrics: vec![
                            TemplateMetric::new_template_metric("x".into(), 0_i32),
                            TemplateMetric::new_template_metric("y".into(), 0_i32),
                            TemplateMetric::new_template_metric("z".into(), 0_i32)
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

    let valid_instance = || {
        TemplateInstance {
            version: None,
            metrics: vec![
                TemplateMetric::new_template_metric("first".into(), 1),
                TemplateMetric::new_template_metric(
                    "nested".into(),
                    TemplateInstance {
                        version: Simple::template_version().map(String::from),
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
    };

    assert_eq!(
        nested.template_instance(),
        valid_instance() 
    );

    assert!(
        NestedTemplate::try_from(
            valid_instance()
        ).is_ok()
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
