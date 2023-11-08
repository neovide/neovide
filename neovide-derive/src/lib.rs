use proc_macro::TokenStream;
use quote::quote;
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
    let fragments = data.fields.iter().map(|field| match field.ident {
        Some(ref ident) => {
            let vim_setting_name = format!("{prefix}{ident}");
            let field_name = format!("{ident}");

            let location = match option(&field) {
                Ok(option) => match option {
                    Some(option_name) => quote! {{ crate::settings::SettingLocation::NeovimOption(#option_name.to_owned()) }},
                    None => quote! {{ crate::settings::SettingLocation::NeovideGlobal(#vim_setting_name.to_owned()) }},
                },
                Err(error) => {
                    return error.to_compile_error();
                }
            };

            quote! {{
                fn update(value: rmpv::Value) {
                    let mut s = crate::settings::SETTINGS.get::<#name>();
                    s.#ident.parse_from_value(value);
                    crate::settings::SETTINGS.set(&s);
                    crate::event_aggregator::EVENT_AGGREGATOR.send(
                        crate::settings::SettingChanged::<#name>::new(#field_name)
                    );
                }

                crate::settings::SETTINGS.set_setting_handlers(
                    #location,
                    update,
                );
            }}
        }
        None => {
            Error::new_spanned(field.colon_token, "Expected named struct fields").to_compile_error()
        }
    });
    let expanded = quote! {
        impl #name {
            pub fn register() {
                let s: Self = Default::default();
                crate::settings::SETTINGS.set(&s);
                #(#fragments)*
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
