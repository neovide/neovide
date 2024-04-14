//! Derive macro for setting groups.
//!
//! This macro will generate a `SettingGroup` implementation for the struct it is applied to.
//! It will also generate an enum with the name `{StructName}Changed` that contains a variant for
//! each field in the struct. The enum will be used to send events when a setting is changed.

use convert_case::{Case, Casing};
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, Attribute, Data, DataStruct, DeriveInput, Error, Field, Ident, Lit, Meta,
};

#[proc_macro_derive(SettingGroup, attributes(setting_prefix, option))]
pub fn setting_group(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let prefix = setting_prefix(input.attrs.as_ref())
        .map(|p| format!("{p}_"))
        .unwrap_or_default();
    stream(input, prefix)
}

fn stream(input: DeriveInput, prefix: String) -> TokenStream {
    const ERR_MSG: &str = "Derive macro expects a struct";
    match input.data {
        Data::Struct(ref data) => struct_stream(input.ident, prefix, data),
        Data::Enum(data) => Error::new_spanned(data.enum_token, ERR_MSG)
            .to_compile_error()
            .into(),
        Data::Union(data) => Error::new_spanned(data.union_token, ERR_MSG)
            .to_compile_error()
            .into(),
    }
}

fn struct_stream(name: Ident, prefix: String, data: &DataStruct) -> TokenStream {
    let event_name = format_ident!("{}Changed", name);
    let name_without_settings = Ident::new(&name.to_string().replace("Settings", ""), name.span());

    let listener_fragments = data.fields.iter().map(|field| match field.ident {
        Some(ref ident) => {
            let vim_setting_name = format!("{prefix}{ident}");

            let option_name = match option(field) {
                Ok(option_name) => option_name,
                Err(error) => {
                    return error.to_compile_error();
                }
            };

            let location = match &option_name {
                Some(option_name) => quote! {{ crate::settings::SettingLocation::NeovimOption(#option_name.to_owned()) }},
                None => quote! {{ crate::settings::SettingLocation::NeovideGlobal(#vim_setting_name.to_owned()) }},
            };

            let field_ident = field.ident.as_ref().unwrap();
            let case_name = field_ident.to_string().to_case(Case::Pascal);
            let case_ident = Ident::new(&case_name, field_ident.span());

            // Only create a reader function for global neovide variables
            let reader = if option_name.is_none() {
                quote! {
                    fn reader(settings: &crate::settings::Settings) -> Option<rmpv::Value> {
                        let s = settings.get::<#name>();
                        Some(s.#ident.into())
                    }
                }
            } else {
                quote! {
                    fn reader(_settings: &crate::settings::Settings) -> Option<rmpv::Value> {
                        None
                    }
                }
            };

            quote! {{
                fn update(settings: &crate::settings::Settings, value: rmpv::Value) -> crate::settings::SettingsChanged {
                    let mut s = settings.get::<#name>();
                    s.#ident.parse_from_value(value);
                    settings.set(&s);
                    #event_name::#case_ident(s.#ident.clone()).into()
                }

                #reader

                settings.set_setting_handlers(
                    #location,
                    update,
                    reader,
                );
            }}
        }
        None => {
            Error::new_spanned(field.colon_token, "Expected named struct fields").to_compile_error()
        }
    });

    let updated_case_fragments = data.fields.iter().map(|field| {
        let field_ident = field.ident.as_ref().unwrap();
        let case_name = field_ident.to_string().to_case(Case::Pascal);
        let case_ident = Ident::new(&case_name, field_ident.span());
        let ty = field.ty.clone();
        quote! {
            #case_ident(#ty),
        }
    });
    let expanded = quote! {
        #[derive(Debug, Clone, PartialEq, strum::AsRefStr)]
        pub enum #event_name {
            #(#updated_case_fragments)*
        }

        impl crate::settings::SettingGroup for #name {
            type ChangedEvent = #event_name;
            fn register(settings: &crate::settings::Settings) {
                let s: Self = Default::default();
                settings.set(&s);
                #(#listener_fragments)*
            }
        }

        impl From<#event_name> for crate::settings::SettingsChanged {
            fn from(value: #event_name) -> Self {
                crate::settings::SettingsChanged::#name_without_settings(value)
            }
        }
    };
    TokenStream::from(expanded)
}

fn setting_prefix(attrs: &[Attribute]) -> Option<String> {
    for attr in attrs.iter() {
        if let Ok(Meta::NameValue(name_value)) = attr.parse_meta() {
            if name_value.path.is_ident("setting_prefix") {
                if let Lit::Str(literal) = name_value.lit {
                    return Some(literal.value());
                }
            }
        }
    }
    None
}

fn option(field: &Field) -> Result<Option<String>, Error> {
    for attr in field.attrs.iter() {
        if !attr.path.is_ident("option") {
            continue;
        }

        if let Ok(Meta::NameValue(name_value)) = attr.parse_meta() {
            if let Lit::Str(literal) = name_value.lit {
                return Ok(Some(literal.value()));
            }
        }
        return Err(Error::new_spanned(
            attr,
            "Expected a string literal for option attribute",
        ));
    }

    Ok(None)
}
