use srad::types::{
    PartialTemplate, Template, TemplateInstance, TemplateMetadata,
    TemplateMetric, TemplateParameter
};

#[derive(Template, Clone, Default)]
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
    let update_b_and_d = TemplateInstance {
        version: None,
        metrics: vec![
            TemplateMetric::new_template_metric("b".into(), 4),
        ],
        parameters: vec![
            TemplateParameter::new_template_parameter("d".into(), 5),
        ],
        template_ref: PartialTest::template_definition_metric_name()
    };
    simple1.update_from_instance(update_b_and_d).unwrap();
    assert_eq!(simple1.b, 4);
    assert_eq!(simple1.d, 5);
}

#[test]
fn test_template_instance_from_difference_nested()
{

}

#[test]
fn test_update_from_instance_nested()
{

}
