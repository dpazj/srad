use srad::types::{
    PartialTemplate, Template, TemplateError, TemplateInstance, TemplateMetadata, TemplateMetric, TemplateParameter
};

#[derive(Template, Clone, Default, PartialEq, Debug)]
struct PartialTest {
    a: i32,
    b: i32,
    c: i32,
    #[template(parameter)]
    d: i32
}

impl TemplateMetadata for PartialTest {
    fn template_name() -> &'static str {
        "test"
    }
}

#[test]
fn test_template_instance_from_difference()
{
    let simple1 = PartialTest { a: 1, b: 2, c: 3, d: 4};
    //Test fields are not included in the difference when they should not be
    let mut simple2 = simple1.clone();
    simple2.b = 4;
    assert_eq!(
        simple2.template_instance_from_difference(&simple1),
        Some(TemplateInstance {
            version: None,
            metrics: vec![
                TemplateMetric::new_template_metric("b".into(), 4),
            ],
            parameters: vec![],
            template_ref: PartialTest::template_definition_metric_name()
        })
    );

    //Test parameters are included in the difference when they should be
    simple2.b = simple1.b;
    simple2.d = 5;
    assert_eq!(
        simple2.template_instance_from_difference(&simple1),
        Some(TemplateInstance {
            version: None,
            metrics: vec![],
            parameters: vec![
                TemplateParameter::new_template_parameter("d".into(), 5),
            ],
            template_ref: PartialTest::template_definition_metric_name()
        })
    );

    //test no difference 
    assert_eq!(
        simple1.template_instance_from_difference(&simple1),
        None
    );
    let simple3 = simple1.clone();
    assert_eq!(
        simple3.template_instance_from_difference(&simple1),
        None
    );
}

#[test]
fn test_update_from_instance()
{
    let mut simple1 = PartialTest { a: 1, b: 2, c: 3, d: 4};
    let valid_update = ||{
        TemplateInstance {
            version: None,
            metrics: vec![
                TemplateMetric::new_template_metric("b".into(), 4),
            ],
            parameters: vec![
                TemplateParameter::new_template_parameter("d".into(), 5),
            ],
            template_ref: PartialTest::template_definition_metric_name()
        }
    };

    let update_b_and_d = valid_update();
    simple1.update_from_instance(update_b_and_d).unwrap();
    assert_eq!(simple1, PartialTest { a: 1, b: 4, c: 3, d: 5});

    let mut invalid_ref = valid_update();
    invalid_ref.template_ref = "INVALID".into();
    assert!(matches!(
        simple1.update_from_instance(
            invalid_ref
        ),
        Err(TemplateError::RefMismatch(_))
    ));

    let mut unknown_metric= valid_update();
    unknown_metric.metrics.push(TemplateMetric::new_template_metric("UNKNOWN".into(), 0));
    assert!(matches!(
        simple1.update_from_instance(
            unknown_metric 
        ),
        Err(TemplateError::UnknownMetric(_))
    ));

    let mut unknown_param= valid_update();
    unknown_param.parameters.push(TemplateParameter::new_template_parameter("UNKNOWN".into(), 0));
    assert!(matches!(
        simple1.update_from_instance(
           unknown_param 
        ),
        Err(TemplateError::UnknownParameter(_))
    ));

}

#[derive(Template, Clone, Default, PartialEq, Debug)]
struct PartialNestedTest {
    a: i32,
    #[template(parameter)]
    b: i32,
    c: PartialTest
}

impl TemplateMetadata for PartialNestedTest {
    fn template_name() -> &'static str {
        "nested_test"
    }
}

#[test]
fn test_template_instance_from_difference_nested()
{
    let nested =  PartialNestedTest { a: 2, b: 4, c: PartialTest { a: 1, b: 2, c: 3, d: 4}};
    assert_eq!(
        nested.template_instance_from_difference(&nested),
        None
    );

    let mut nested1 = nested.clone();
    nested1.a = 4;
    nested1.c.a = 2;
    assert_eq!(
        nested1.template_instance_from_difference(&nested),
        Some(TemplateInstance {
            version: None,
            metrics: vec![
                TemplateMetric::new_template_metric("a".into(), 4_i32),
                TemplateMetric::new_template_metric(
                    "c".into(),
                    TemplateInstance {
                        version: None,
                        metrics: vec![
                            TemplateMetric::new_template_metric("a".into(), 2_i32),
                        ],
                        parameters: vec![],
                        template_ref: PartialTest::template_definition_metric_name()
                    }
                ),
            ],
            parameters: vec![],
            template_ref: PartialNestedTest::template_definition_metric_name()
        })
    );

}

#[test]
fn test_update_from_instance_nested()
{
    let mut nested =  PartialNestedTest { a: 2, b: 4, c: PartialTest { a: 1, b: 2, c: 3, d: 4}};
    let instance = TemplateInstance {
        version: None,
        metrics: vec![
            TemplateMetric::new_template_metric("a".into(), 4_i32),
            TemplateMetric::new_template_metric(
                "c".into(),
                TemplateInstance {
                    version: None,
                    metrics: vec![
                        TemplateMetric::new_template_metric("a".into(), 2_i32),
                    ],
                    parameters: vec![],
                    template_ref: PartialTest::template_definition_metric_name()
                }
            ),
        ],
        parameters: vec![],
        template_ref: PartialNestedTest::template_definition_metric_name()
    };
    nested.update_from_instance(instance).unwrap();
    assert_eq!(nested, PartialNestedTest { a: 4, b: 4, c: PartialTest { a: 2, b: 2, c: 3, d: 4}});
}
