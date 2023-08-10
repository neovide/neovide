use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Attribute, Data, DataStruct, DeriveInput, Error, Ident, Lit, Meta};

#[proc_macro_derive(SettingGroup, attributes(setting_prefix))]
pub fn setting_group(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let prefix = setting_prefix(input.attrs.as_ref())
        .map(|p| format!("{p}_"))
        .unwrap_or_else(|| "".to_string());
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

fn first_part_of_path(ty: &syn::Type) -> quote::__private::TokenStream {
    if let syn::Type::Path(p) = ty {
        let ident = &p.path.segments.first().unwrap().ident;
        quote! {
            #ident
        }
    } else {
        quote! {
            #ty
        }
    }
}

fn struct_stream(name: Ident, prefix: String, data: &DataStruct) -> TokenStream {
    let fragments = data.fields.iter().map(|field| match field.ident {
        Some(ref ident) => {
            let vim_setting_name = format!("{prefix}{ident}");
            let raw_type = &field.ty;
            let first = first_part_of_path(&field.ty);

            quote! {{
                fn update_func(value: rmpv::Value) {
                    let mut s = crate::settings::SETTINGS.get::<#name>();
                    let new_value: Result<#raw_type, _> = #first::parse_from_value(&value);
                    match new_value {
                        Ok(new_value) => {
                            s.#ident = new_value;
                            crate::settings::SETTINGS.set(&s);
                        }
                        Err(e) => {
                            warn!("Failed to parse setting value for {}", e);
                        }
                    }
                }

                fn reader_func() -> rmpv::Value {
                    let s = crate::settings::SETTINGS.get::<#name>();
                    s.#ident.convert_into_value()
                }

                crate::settings::SETTINGS.set_setting_handlers(
                    #vim_setting_name,
                    update_func,
                    reader_func
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
