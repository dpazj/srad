use srad::types::{Template, PartialTemplate, TemplateMetadata};

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
    let simple = Simple::default();
}


#[derive(Template)]
struct NestedTemplate {
    first: i32,
    nested: Simple
}

impl TemplateMetadata for NestedTemplate {
    fn template_name() -> &'static str {
        "nested"
    }
}

#[test]
pub fn test_nested() {
    todo!()
}

#[test]
pub fn test_default() {
    todo!()
}

#[test]
pub fn test_skip() {
    todo!()
}

#[test]
pub fn test_rename() {
    todo!()
}

#[test]
pub fn test_parameter() {
    todo!()
}


