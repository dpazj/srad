use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput};


fn try_template(input: DeriveInput) -> proc_macro::TokenStream {

    // let data_struct = match input.data {
    //     Data::Struct(variant_data) => variant_data,
    //     Data::Enum(..) => panic!("Template can not be derived for an enum"),
    //     Data::Union(..) => panic!("Template can not be derived for a union"),
    // };

    // let fields= match data_struct.fields {
    //     syn::Fields::Named(fields_named) => fields_named,
    //     _ => panic!("Template can only be derived for structs with named fields"),
    // };

    let name = &input.ident;

    quote!{
        impl Template for #name {

            fn template_definition() -> TemplateDefinition 
            {
                todo!()
            }

            fn template_instance(&self) -> TemplateInstance 
            {
                todo!()
            }

        }
    }.into()
}


#[proc_macro_derive(Template)]
pub fn template_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream{
    let input = parse_macro_input!(input as DeriveInput);
    try_template(input)
}
