use srad::types::{Template, TemplateMetadata};

#[derive(Default, Debug, Clone, Template, PartialEq)]
struct NestedTest {
    one: u32,
    two: u64,
}

impl TemplateMetadata for NestedTest {
    fn template_name() -> &'static str {
        "nested_test"
    }
}

#[derive(Template, Debug, PartialEq)]
struct Test {
    #[template(default = 12, parameter)]
    x: i32,
    y: i32,
    z: Option<i32>,
    #[template(rename = "A")]
    renamed: i32,
    nested: NestedTest,
    #[template(skip)]
    ignore: i32,
}

impl TemplateMetadata for Test {
    fn template_name() -> &'static str {
        "test"
    }
}

#[test]
pub fn test() {
    let mut a = Test {
        x: 1,
        y: 1,
        z: Some(69),
        renamed: 23,
        ignore: Default::default(),
        nested: Default::default(),
    };
    let b = Test {
        x: 2,
        y: 1,
        z: Some(70),
        renamed: 0,
        ignore: 1234,
        nested: NestedTest { one: 1, two: 0 },
    };

    let x = Test::template_definition();
    println!("{x:#?}");

    let y = a.template_instance();
    println!("{y:#?}");

    let a_from_instance = Test::try_from(y).unwrap();
    println!("A from instance {a_from_instance:#?}");

    assert!(a_from_instance == a);

    // let template_instance_diff = a.template_instance_from_difference(&b);
    // println!("{template_instance_diff:#?}");

    // a.update_from_instance(template_instance_diff.unwrap())
    //     .unwrap();

    panic!("A")
}

