use std::collections::HashSet;

use proc_macro2;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, Attribute, Data, DataEnum, DataUnion, DeriveInput, Error};

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
                return Ok(());
            }

            if meta.path.is_ident("parameter") {
                field_attrs.parameter = true;
                return Ok(());
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
        Data::Enum(DataEnum { enum_token, .. }) => {
            return Err(Error::new_spanned(
                enum_token,
                "Template can not be derived for an Enum",
            ))
        }
        Data::Union(DataUnion { union_token, .. }) => {
            return Err(Error::new_spanned(
                union_token,
                "Template can not be derived for a Union",
            ))
        }
    };

    let fields = match data_struct.fields {
        syn::Fields::Named(fields_named) => fields_named,
        syn::Fields::Unnamed(fields_unnamed) => {
            return Err(Error::new_spanned(
                fields_unnamed,
                "Template can not be derived for a tuple struct with Unnamed Fields",
            ))
        }
        syn::Fields::Unit => {
            return Err(Error::new_spanned(
                &data_struct.fields,
                "Template does not support unit structs",
            ))
        }
    };

    let mut unique_names = HashSet::with_capacity(fields.named.len());

    let mut definition_metrics = Vec::new();
    let mut definition_parameters = Vec::new();

    let mut instance_metrics = Vec::new();
    let mut instance_parameters = Vec::new();

    let mut from_difference_metrics = Vec::new();
    let mut from_difference_parameters = Vec::new();

    let mut from_instance_defines = Vec::new();
    let mut from_instance_metric_match = Vec::new();
    let mut from_instance_parameter_match = Vec::new();
    let mut from_instance_init_struct = Vec::new();

    let mut update_from_instance_init_defines = Vec::new();
    let mut update_from_instance_metric_match = Vec::new();
    let mut update_from_instance_parameter_match = Vec::new();
    let mut update_from_instance_update_defines = Vec::new();

    for field in &fields.named {
        let attrs = parse_builder_attributes(&field.attrs)?;
        let field_ident = &field.ident;
        let ty = field.ty.clone();
        let name = if let Some(rename) = attrs.rename {
            rename
        } else {
            field.ident.as_ref().unwrap().to_string()
        };

        let default = if let Some(default) = attrs.default {
            default
        } else {
            quote! { <#ty as Default>::default() }
        };

        from_instance_defines.push(quote! {
            let mut #field_ident = #default;
        });

        from_instance_init_struct.push(quote! {
            #field_ident,
        });

        if attrs.skip {
            continue;
        }

        if !unique_names.insert(name.clone()) {
            return Err(Error::new_spanned(
                field.ident.clone(),
                format!("Duplicate name provided - {name}"),
            ));
        }

        update_from_instance_init_defines.push(quote! {
            let mut #field_ident = None;
        });

        update_from_instance_update_defines.push(quote! {
            if let Some(v) = #field_ident {
                self.#field_ident = v;
            }
        });

        if attrs.parameter {
            definition_parameters.push(quote! {
                ::srad::types::TemplateParameter::new_template_parameter::<#ty>(
                    #name.to_string(),
                    #default
                )
            });

            instance_parameters.push(quote! {
                ::srad::types::TemplateParameter::new_template_parameter(
                    #name.to_string(),
                    self.#field_ident.clone()
                )
            });

            from_instance_parameter_match.push(
                quote! {
                    #name => {
                        #field_ident = match ::srad::types::TemplateParameterValue::try_from_template_parameter_value(
                            parameter.value.map(::srad::types::ParameterValue::from)
                        ) {
                            Ok(v) => v,
                            Err(_) => return Err(::srad::types::TemplateError::InvalidParameterValue(#name.to_string()))
                        }
                    },
                }
            );

            from_difference_parameters.push(quote! {
                if self.#field_ident != other.#field_ident {
                    parameters.push(
                        ::srad::types::TemplateParameter::new_template_parameter(
                            #name.to_string(),
                            self.#field_ident.clone()
                        )
                    )
                }
            });

            update_from_instance_parameter_match.push(
                quote! {
                    #name => {
                        #field_ident = match ::srad::types::TemplateParameterValue::try_from_template_parameter_value(
                            parameter.value.map(::srad::types::ParameterValue::from)
                        ) {
                            Ok(v) => Some(v),
                            Err(_) => return Err(::srad::types::TemplateError::InvalidParameterValue(#name.to_string()))
                        };
                    },
                }
            );

        } else {
            definition_metrics.push(quote! {
                ::srad::types::TemplateMetric::new_template_metric::<#ty>(
                    #name.to_string(),
                    #default
                )
            });

            instance_metrics.push(quote! {
                ::srad::types::TemplateMetric::new_template_metric(
                    #name.to_string(),
                    self.#field_ident.clone()
                )
            });

            from_instance_metric_match.push(
                quote! {
                    #name => {
                        #field_ident = match ::srad::types::TemplateMetricValue::try_from_template_metric_value(
                            metric.value.map(::srad::types::MetricValue::from)
                        ) {
                            Ok(v) => v,
                            Err(e) => return Err(::srad::types::TemplateError::InvalidMetricValue(#name.to_string()))
                        }
                    },
                }
            );

            from_difference_metrics.push(
                quote! {
                    if let Some(value) = ::srad::types::TemplateMetricValuePartial::metric_value_if_ne(&self.#field_ident, &other.#field_ident) {
                        metrics.push(
                            ::srad::types::TemplateMetric::new_template_metric_raw(
                                #name.to_string(),
                                <#ty as ::srad::types::traits::HasDataType>::default_datatype(),
                                value
                            )
                        )
                    }
                }
            );

            update_from_instance_metric_match.push(
                quote! {
                    #name => {
                        let mut tmp = self.#field_ident.clone();
                        if let Err(_) = ::srad::types::TemplateMetricValuePartial::try_update_from_metric_value(
                            &mut tmp,
                            metric.value.map(::srad::types::MetricValue::from)
                        ) {
                            return Err(::srad::types::TemplateError::InvalidMetricValue(#name.to_string()))
                        }
                        #field_ident = Some(tmp);
                    },
                }
            );
        }
    }

    if unique_names.is_empty() {
        return Err(Error::new_spanned(
            fields,
            format!("At least one field must be provided"),
        ));
    }

    let type_name = &input.ident;

    Ok(quote!{

        impl ::srad::types::TemplateMetricValue for #type_name {
            fn to_template_metric_value(self) -> Option<::srad::types::MetricValue> {
                Some(::srad::types::Template::template_instance(&self).into())
            }
            fn try_from_template_metric_value(value: Option<::srad::types::MetricValue>) -> Result<Self, ()> where Self: Sized {
                match value {
                    Some(value) => {
                        Self::try_from(
                            ::srad::types::TemplateInstance::try_from(value)
                            .map_err(|_|())?
                        )
                        .map_err(|_|())
                    },
                    None => Err(()),
                }
            }
        }

        impl ::srad::types::TemplateMetricValuePartial for #type_name {
            fn metric_value_if_ne(&self, other: &Self) -> Option<Option<::srad::types::MetricValue>> {
                if let Some(difference_instance) = ::srad::types::PartialTemplate::template_instance_from_difference(self, other)
                {
                    return Some(Some(difference_instance.into()))
                }
                return None
            }
            fn try_update_from_metric_value(&mut self, value: Option<::srad::types::MetricValue>) -> Result<(), ()> {
                let instance = match value {
                    Some(val) => ::srad::types::TemplateInstance::try_from(val).map_err(|_|())?,
                    None => return Ok(())
                };
                ::srad::types::PartialTemplate::update_from_instance(self, instance).map_err(|_|())
            }
        }

        impl ::srad::types::Template for #type_name {

            fn template_definition() -> ::srad::types::TemplateDefinition
            {
                let parameters = vec![
                    #(#definition_parameters),*
                ];
                let metrics = vec![
                    #(#definition_metrics),*
                ];
                ::srad::types::TemplateDefinition {
                    version: Self::template_version().map(|version| version.to_owned()),
                    metrics,
                    parameters
                }
            }

            fn template_instance(&self) -> ::srad::types::TemplateInstance
            {
                let parameters = vec![
                   #(#instance_parameters),*
                ];
                let metrics = vec![
                   #(#instance_metrics),*
                ];
                ::srad::types::TemplateInstance {
                    template_ref: Self::template_definition_metric_name().to_owned(),
                    version: Self::template_version().map(|version| version.to_owned()),
                    metrics,
                    parameters
                }
            }
        }

        impl ::srad::types::PartialTemplate for #type_name {

            fn template_instance_from_difference(&self, other: &Self) -> Option<::srad::types::TemplateInstance>
            {
                let mut parameters = Vec::new();
                let mut metrics = Vec::new();

                #(#from_difference_metrics)*
                #(#from_difference_parameters)*

                if parameters.is_empty() && metrics.is_empty() {
                    return None
                }

                Some(::srad::types::TemplateInstance{
                    template_ref: Self::template_definition_metric_name().to_owned(),
                    version: Self::template_version().map(|version| version.to_owned()),
                    metrics,
                    parameters
                })
            }

            fn update_from_instance(&mut self, instance: ::srad::types::TemplateInstance) -> Result<(), ::srad::types::TemplateError> {

                if instance.template_ref != Self::template_definition_metric_name() {
                    return Err(::srad::types::TemplateError::RefMismatch(instance.template_ref))
                }

                if instance.version.as_deref() != Self::template_version() {
                    return Err(::srad::types::TemplateError::VersionMismatch)
                }

                #(#update_from_instance_init_defines)*

                for parameter in instance.parameters {
                    let name = parameter.name.ok_or(::srad::types::TemplateError::InvalidPayload)?;
                    match name.as_str() {
                        #(#update_from_instance_parameter_match)*
                        _ => return Err(::srad::types::TemplateError::UnknownParameter(name))
                    }
                }

                for metric in instance.metrics {
                    let name = metric.name.ok_or(::srad::types::TemplateError::InvalidPayload)?;
                    match name.as_str() {
                        #(#update_from_instance_metric_match)*
                        _ => return Err(::srad::types::TemplateError::UnknownMetric(name))
                    }
                }

                #(#update_from_instance_update_defines)*

                Ok(())
            }

        }

        impl TryFrom<::srad::types::TemplateInstance> for #type_name {

            type Error = ::srad::types::TemplateError;

            fn try_from(value: ::srad::types::TemplateInstance) -> Result<Self, Self::Error> {
                if value.template_ref != Self::template_definition_metric_name() {
                    return Err(::srad::types::TemplateError::RefMismatch(value.template_ref))
                }

                if value.version.as_deref() != Self::template_version() {
                    return Err(::srad::types::TemplateError::VersionMismatch)
                }

                #(#from_instance_defines)*

                for parameter in value.parameters {
                    let name = parameter.name.ok_or(::srad::types::TemplateError::InvalidPayload)?;
                    match name.as_str() {
                        #(#from_instance_parameter_match)*
                        _ => return Err(::srad::types::TemplateError::UnknownParameter(name))
                    }
                }

                for metric in value.metrics {
                    let name = metric.name.ok_or(::srad::types::TemplateError::InvalidPayload)?;
                    match name.as_str() {
                        #(#from_instance_metric_match)*
                        _ => return Err(::srad::types::TemplateError::UnknownMetric(name))
                    }
                }

                Ok(Self {
                    #(#from_instance_init_struct)*
                })
            }

        }

    }.into())
}

/// # Template Derive Macro
///
/// The `#[derive(Template)]` macro provides automatic implementation of the `Template`,
/// `PartialTemplate`, `TemplateMetricValuePartial` and `TemplateMetricValue` traits.
///
/// ## Requirements
///
/// - **Struct with named fields**: The macro only works with structs that have named fields
/// - **TemplateMetadata implementation**: You must manually implement `TemplateMetadata` for your struct
/// - All metric fields must implement `TemplateMetricValue`, `TemplateMetricValuePartial`.
/// - All fields must implement `Clone`.
/// - If no default attribute value is provided, fields must implement `Default`
///
/// ## Basic Usage
///
/// ```rust
/// # use srad::types::{Template, TemplateMetadata};
/// #[derive(Template)]
/// struct SensorData {
///     temperature: f64,
///     humidity: f64,
///     battery_level: u8,
///     timestamp: u64,
/// }
///
/// impl TemplateMetadata for SensorData {
///     fn template_name() -> &'static str {
///         "sensor_data"
///     }
///     
///     fn template_version() -> Option<&'static str> {
///         Some("1.0.0")
///     }
/// }
///
/// // Template and PartialTemplate are automatically implemented!
/// ```
///
/// ## Field Attributes
///
/// The macro supports several attributes to customise field behavior:
///
/// ### `#[template(skip)]`
///
/// Excludes a field from the template definition and instances.
///
/// ### `#[template(parameter)]`
///
/// Marks a field as a template parameter rather than as a metric.  
///
/// ### `#[template(default = value)]`
///
/// Provides a default value for a field in the template definition. If not provided then [Default::default()]
/// trait is used.
///
/// ### `#[template(rename = "new_name")]`
///
/// Changes the metric or parameter name in the template definition. By default, the field
/// name is used. Names must be unique.
///
/// ## Example
///
/// ```rust
/// use srad::types::{Template, PartialTemplate, TemplateMetadata};
/// use std::collections::HashMap;
///
/// #[derive(Template, Clone)]
/// struct MotorController {
///     #[template(rename = "rpm")]
///     revolutions_per_minute: f64,
///     
///     #[template(rename = "temp_c")]
///     temperature_celsius: f64,
///     
///     #[template(parameter, default = 3000.0)]
///     max_rpm: f64,
///     
///     #[template(parameter, default = 85.0)]
///     temp_warning_threshold: f64,
///     
///     #[template(default = false)]
///     alarm_active: bool,
///     
///     #[template(skip)]
///     internal_diagnostics: HashMap<String, String>,
/// }
///
/// impl TemplateMetadata for MotorController {
///     fn template_name() -> &'static str {
///         "motor_controller"
///     }
///     
///     fn template_version() -> Option<&'static str> {
///         Some("2.1.0")
///     }
/// }
///
/// // Usage
/// let motor = MotorController {
///     revolutions_per_minute: 2500.0,
///     temperature_celsius: 72.0,
///     max_rpm: 3000.0,
///     temp_warning_threshold: 85.0,
///     alarm_active: false,
///     internal_diagnostics: HashMap::new(),
/// };
///
/// // Get template definition
/// let definition = MotorController::template_definition();
///
/// // Create instance
/// let instance = motor.template_instance();
///
/// // Create differential update
/// let updated_motor = MotorController {
///     revolutions_per_minute: 2750.0,  // Changed
///     temperature_celsius: 72.0,       // Same
///     max_rpm: 3000.0,
///     temp_warning_threshold: 85.0,
///     alarm_active: false,
///     internal_diagnostics: HashMap::new(),
/// };
///
/// if let Some(diff) = updated_motor.template_instance_from_difference(&motor) {
///     // diff contains only the 'rpm' metric
/// }
/// ```
#[proc_macro_derive(Template, attributes(template))]
pub fn template_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match try_template(input) {
        Ok(tokens) => tokens,
        Err(err) => err.into_compile_error().into(),
    }
}
