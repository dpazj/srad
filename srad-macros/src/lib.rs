use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput};


fn try_template(input: DeriveInput) -> TokenStream {

    let data_struct = match input.data {
        Data::Struct(variant_data) => variant_data,
        Data::Enum(..) => panic!("Template can not be derived for an enum"),
        Data::Union(..) => panic!("Template can not be derived for a union"),
    };

    let fields= match data_struct.fields {
        syn::Fields::Named(fields_named) => fields_named,
        _ => panic!("Template can only be derived for structs with named fields"),
    };




    


    todo!() 
}


#[proc_macro_derive(Template)]
pub fn template_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    try_template(input)
}
