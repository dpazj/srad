use quote::{quote, ToTokens};
use syn::{parse_macro_input, Attribute, Data, DeriveInput, Type, TypePath};
use proc_macro2;

/// TODO 
/// - attributes 
///     - default
///     - rename 
///     - is parameter
///     - metric type override 
///   

#[derive(Default)]
struct TemplateFieldAttributes {
    skip: bool,
    parameter: bool,
    rename: Option<String>,
    default: Option<proc_macro2::TokenStream>,
}

fn parse_builder_attributes(attrs: &[Attribute]) -> syn::Result<TemplateFieldAttributes> {
    let mut field_attrs = TemplateFieldAttributes::default();
    
    for attr in attrs {
        if !attr.path().is_ident("template") {
            continue;
        }

        attr.parse_nested_meta(|meta| {

            if meta.path.is_ident("skip") {
                field_attrs.skip = true;
                return Ok(())
            }

            if meta.path.is_ident("parameter") {
                field_attrs.parameter = true;
                return Ok(())
            }

            if meta.path.is_ident("default") {
                let value = meta.value()?;
                let expr: syn::Expr = value.parse()?;
                field_attrs.default = Some(expr.into_token_stream());
                return Ok(());
            }

            Err(meta.error("Unrecognised template attribute"))
        })?;
    }
    
    Ok(field_attrs)
}

fn try_template(input: DeriveInput) -> syn::Result<proc_macro::TokenStream> {

    let data_struct = match input.data {
        Data::Struct(variant_data) => variant_data,
        Data::Enum(..) => panic!("Template can not be derived for an enum"),
        Data::Union(..) => panic!("Template can not be derived for a union"),
    };

    let fields = match data_struct.fields {
        syn::Fields::Named(fields_named) => fields_named,
        _ => panic!("Template can only be derived for structs with named fields"),
    };

    let mut definition_metrics = Vec::new();
    let mut definition_parameters= Vec::new();

    let mut instance_metrics = Vec::new();
    let mut instance_parameters = Vec::new();

    for field in fields.named {

        let field_ident = &field.ident;
        let name = field.ident.as_ref().unwrap().to_string();
        let ty = field.ty.clone();
        let attrs = parse_builder_attributes(&field.attrs)?;

        if attrs.skip { continue }

        let default = if let Some(default) = attrs.default {
            default
        } else {
            quote! { <#ty as Default>::default() }
        };

        if attrs.parameter {

            definition_parameters.push(
                quote! {
                    ::srad_types::TemplateParameter::new_template_parameter::<#ty>(
                        #name.to_string(), 
                        #default 
                    )
                }
            );

            instance_parameters.push(
                quote! {
                    ::srad_types::TemplateParameter::new_template_parameter(
                        #name.to_string(), 
                        self.#field_ident.clone()
                    )
                }
            );

        }
        else {

            definition_metrics.push(
                quote! {
                    ::srad_types::TemplateMetric::new_template_metric::<#ty>(
                        #name.to_string(), 
                        #default
                    )
                }
            );

            instance_metrics.push(
                quote! {
                    ::srad_types::TemplateMetric::new_template_metric(
                        #name.to_string(), 
                        self.#field_ident.clone()
                    )
                }
            );

        }

    }

    let type_name = &input.ident;

    Ok(quote!{
        impl Template for #type_name {

            fn template_definition() -> ::srad_types::TemplateDefinition 
            {
                let parameters = vec![
                    #(#definition_parameters),*
                ];
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
                let parameters = vec![
                   #(#instance_parameters),*
                ];
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
    }.into())
}

#[proc_macro_derive(Template, attributes(template))]
pub fn template_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream{
    let input = parse_macro_input!(input as DeriveInput);
    match try_template(input) {
        Ok(tokens) => tokens,
        Err(err) => err.into_compile_error().into(),
    }
}
