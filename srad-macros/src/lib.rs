use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Type, TypePath};

/// TODO 
/// - attributes 
///     - default
///     - rename 
///     - is parameter
///     - metric type override 
///   


fn try_template(input: DeriveInput) -> proc_macro::TokenStream {

    let data_struct = match input.data {
        Data::Struct(variant_data) => variant_data,
        Data::Enum(..) => panic!("Template can not be derived for an enum"),
        Data::Union(..) => panic!("Template can not be derived for a union"),
    };

    let fields = match data_struct.fields {
        syn::Fields::Named(fields_named) => fields_named,
        _ => panic!("Template can only be derived for structs with named fields"),
    };

    let instance_metrics = fields.named.iter().map(|x| {
        let field = &x.ident;
        let name = x.ident.as_ref().unwrap().to_string();
        quote! {
            ::srad_types::TemplateMetric::new_template_metric(
                #name.to_string(), 
                self.#field.clone()
            )
        }
    });

    let definition_metrics = fields.named.iter().map(|x| {
        let name = x.ident.as_ref().unwrap().to_string();
        let ty = x.ty.clone();
        quote! {
            ::srad_types::TemplateMetric::new_template_metric(
                #name.to_string(), 
                <#ty as Default>::default()
            )
        }
    });

    let name = &input.ident;

    quote!{
        impl Template for #name {

            fn template_definition() -> ::srad_types::TemplateDefinition 
            {
                let parameters = vec![];
                let metrics = vec![
                    #(#definition_metrics),*
                ];
                ::srad_types::TemplateDefinition {
                    name: Self::template_name().to_owned(),
                    version: Self::template_version().map(|version| version.to_owned()),
                    metrics,
                    parameters
                }
            }

            fn template_instance(&self) -> TemplateInstance 
            {
                let parameters = vec![];
                let metrics = vec![
                   #(#instance_metrics),*
                ];
                ::srad_types::TemplateInstance{
                    name: Self::template_name().to_owned(),
                    version: Self::template_version().map(|version| version.to_owned()),
                    metrics,
                    parameters
                }
            }

        }
    }.into()
}

#[proc_macro_derive(Template)]
pub fn template_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream{
    let input = parse_macro_input!(input as DeriveInput);
    try_template(input)
}
