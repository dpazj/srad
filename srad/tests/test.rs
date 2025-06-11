use srad::{types::{Template, TemplateInstance, TemplateDefinition}, Template};
use srad_types::TemplateMetadata;


#[derive(Template)]
struct Test {
    x: i32,
    y: i32,
}

impl TemplateMetadata for Test {
    fn template_name() -> &'static str {
        "test"
    }
}

#[test]
pub fn test(){
    let a = Test { x: 1, y: 1};
    Test::template_definition();
    a.template_instance();
}

// #[derive(Template)]
// enum AA{
//     A,
//     B
// }