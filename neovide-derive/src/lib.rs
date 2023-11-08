use convert_case::{Case, Casing};
use proc_macro::TokenStream;
use quote::{quote, format_ident};
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

    let listener_fragments = data.fields.iter().map(|field| match field.ident {
        Some(ref ident) => {
            let vim_setting_name = format!("{prefix}{ident}");

            let location = match option(field) {
                Ok(option) => match option {
                    Some(option_name) => quote! {{ crate::settings::SettingLocation::NeovimOption(#option_name.to_owned()) }},
                    None => quote! {{ crate::settings::SettingLocation::NeovideGlobal(#vim_setting_name.to_owned()) }},
                },
                Err(error) => {
                    return error.to_compile_error();
                }
            };

            let field_ident = field.ident.as_ref().unwrap();
            let case_name = field_ident.to_string().to_case(Case::Pascal);
            let case_ident = Ident::new(&case_name, field_ident.span());

            quote! {{
                fn update(settings: &crate::settings::Settings, value: rmpv::Value) {
                    let mut s = settings.get::<#name>();
                    s.#ident.parse_from_value(value);
                    settings.set(&s);
                    crate::event_aggregator::EVENT_AGGREGATOR.send(
                        #event_name::#case_ident(s.#ident.clone()),
                    );
                }

                settings.set_setting_handlers(
                    #location,
                    update,
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
        #[derive(Debug, Clone)]
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
