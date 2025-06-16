use srad_types::{Template, TemplateInstance, TemplateMetadata};


#[derive(Template)]
struct Test {
    x: i32,
    y: i32,
    z: Option<i32>,
    #[template(skip)]
    ignore: i32
}

impl TemplateMetadata for Test {
    fn template_name() -> &'static str {
        "test"
    }
}

#[test]
pub fn test(){
    let a = Test { x: 1, y: 1, z: Some(69), ignore: 1234};
    let x = Test::template_definition();

    println!("{x:#?}");
    let y = a.template_instance();
    println!("{y:#?}");
    panic!("A")
}

// #[derive(Template)]
// enum AA{
//     A,
//     B
// }