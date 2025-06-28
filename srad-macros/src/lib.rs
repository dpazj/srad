use std::collections::HashSet;

use quote::{quote, ToTokens};
use syn::{parse_macro_input, Attribute, Data, DataEnum, DataUnion, DeriveInput, Error, Type, TypePath};
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

            if meta.path.is_ident("rename") {
                let value = meta.value()?;
                let lit_string: syn::LitStr = value.parse()?;
                field_attrs.rename = Some(lit_string.value());
                return Ok(())
            }

            Err(meta.error("Unrecognised template attribute"))
        })?;
    }
    
    Ok(field_attrs)
}

fn try_template(input: DeriveInput) -> syn::Result<proc_macro::TokenStream> {

    let data_struct = match input.data {
        Data::Struct(variant_data) => variant_data,
        Data::Enum(DataEnum{ enum_token, .. }) => return Err(Error::new_spanned(enum_token, "Template can not be derived for an Enum")),
        Data::Union(DataUnion { union_token, ..}) => return Err(Error::new_spanned(union_token, "Template can not be derived for a Union")),
    };

    let fields = match data_struct.fields {
        syn::Fields::Named(fields_named) => fields_named,
        syn::Fields::Unnamed(fields_unnamed) => return Err(Error::new_spanned(fields_unnamed, "Template can not be derived for a tuple struct with Unnamed Fields")),
        syn::Fields::Unit => return Err(Error::new_spanned(&data_struct.fields, "Template does not support unit structs")),
    };

    let mut unique_names = HashSet::with_capacity(fields.named.len());

    let mut definition_metrics = Vec::new();
    let mut definition_parameters= Vec::new();

    let mut instance_metrics = Vec::new();
    let mut instance_parameters = Vec::new();

    // let mut from_difference_metrics = Vec::new();
    // let mut from_difference_parameters= Vec::new();

    let mut from_instance_defines = Vec::new();
    let mut from_instance_metric_match = Vec::new();

    for field in fields.named {
        let attrs = parse_builder_attributes(&field.attrs)?;
        let field_ident = &field.ident;
        let ty = field.ty.clone();
        let name = if let Some(rename) = attrs.rename {
            rename
        }
        else {
            field.ident.as_ref().unwrap().to_string()
        };

        if !unique_names.insert(name.clone()) {
            return Err(Error::new_spanned(field.ident, format!("Duplicate name provided - {name}")))
        }

        let default = if let Some(default) = attrs.default {
            default
        } else {
            quote! { <#ty as Default>::default() }
        };

        from_instance_defines.push(
            quote! {
                let mut #field_ident = #default;
            }
        );

        if attrs.skip { continue }

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

            // from_difference_parameters.push(
            //     quote! {
            //         if self.#field_ident != other.#field_ident {
            //             parameters.push(
            //                 ::srad_types::TemplateParameter::new_template_parameter(
            //                     #name.to_string(), 
            //                     self.#field_ident.clone()
            //                 )
            //             )
            //         } 
            //     }
            // );

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

            // from_difference_metrics.push(
            //     quote! {
            //         if let Some(value) = ::srad_types::TemplateMetricValuePartial::metric_value_if_ne(&self.#field_ident, &other.#field_ident) {
            //             metrics.push(
            //                 ::srad_types::TemplateMetric::new_template_metric_raw(
            //                     #name.to_string(), 
            //                     <#ty as ::srad_types::traits::HasDataType>::default_datatype(),
            //                     value
            //                 )
            //             )
            //         }
            //     }
            // );
            from_instance_metric_match.push(
                quote! {
                    #name => {

                        
                        #field_ident
                    }
                }
            );

        }

    }

    let type_name = &input.ident;

    Ok(quote!{

        impl ::srad_types::TemplateMetricValue for #type_name {
            fn to_template_metric_value(self) -> Option<::srad_types::MetricValue> {
                Some(::srad_types::Template::template_instance(&self).into())
            }
        }

        // impl ::srad_types::TemplateMetricValuePartial for #type_name {
        //     fn metric_value_if_ne(&self, other: &Self) -> Option<Option<::srad_types::MetricValue>> {
        //         if let Some(difference_instance) = ::srad_types::Template::template_instance_from_difference(self, other)
        //         {
        //             return Some(Some(difference_instance.into()))
        //         }
        //         return None
        //     }
        // }

        impl ::srad_types::Template for #type_name {

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

            fn template_instance(&self) -> ::srad_types::TemplateInstance 
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
    
            // fn template_instance_from_difference(&self, other: &Self) -> Option<::srad_types::TemplateInstance>
            // {
            //     let mut parameters = Vec::new();
            //     let mut metrics = Vec::new();

            //     #(#from_difference_metrics)*
            //     #(#from_difference_parameters)*

            //     if parameters.is_empty() && metrics.is_empty() { 
            //         return None
            //     }

            //     Some(::srad_types::TemplateInstance{
            //         name: Self::template_name().to_owned(),
            //         version: Self::template_version().map(|version| version.to_owned()),
            //         metrics,
            //         parameters
            //     })
            // }

        }

        impl TryFrom<::srad_types::TemplateInstance> for #type_name {

            type Error = ();

            fn try_from(value: ::srad_types::TemplateInstance) -> Result<Self, Self::Error> {

                if value.name != Self::template_name() { 
                    return Err(())
                }

                if value.version != Self::template_name() {
                    return Err(())
                }

                #(#from_instance_defines)*

                for metric in value.metrics {
                    let name = metric.name.ok_or(())?;
                    let datatype = metric.datatype.ok_or(())?;
                    match name {
                        #(#from_instance_metric_match)*,
                        _ => return Err(())
                    }

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
