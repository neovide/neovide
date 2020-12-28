use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Meta};

#[proc_macro_derive(SettingGroup, attributes(setting_prefix))]
pub fn setting_group(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    match setting_prefix(input.attrs) {
        Some(prefix) => {
            println!("{:?}", prefix);
            let mut functions = vec![];
            let mut register_fragments = vec![];
            match input.data {
                syn::Data::Struct(data) => {
                    for field in data.fields.into_iter() {
                        if let Some(ident) = field.ident {
                            let update_func_name = quote::format_ident!("update_{}", ident);
                            let reader_func_name = quote::format_ident!("reader_{}", ident);
                            functions.push(quote! {
                                fn #update_func_name (value: Value) {
                                    let mut s = crate::settings::SETTINGS.get::<Self>();
                                    s.#ident.from_value(value);
                                    SETTINGS.set(&s);
                                }

                                fn #reader_func_name () -> Value {
                                    let s = crate::settings::SETTINGS.get::<Self>();
                                    s.#ident.into()
                                }
                            });
                            let vim_setting_name = format!("neovide_{}_{}", prefix, ident);
                            register_fragments.push(quote! {
                                crate::settings::SETTINGS.set_setting_handlers(
                                    #vim_setting_name,
                                    Self::#update_func_name,
                                    Self::#reader_func_name
                                );
                            })
                        } else {
                            syn::Error::new_spanned(
                                field.colon_token,
                                "Expected named struct fields",
                            )
                            .to_compile_error();
                        }
                    }
                }
                syn::Data::Enum(data) => {
                    syn::Error::new_spanned(data.enum_token, "Derive macro expects a struct")
                        .to_compile_error();
                }
                syn::Data::Union(data) => {
                    syn::Error::new_spanned(data.union_token, "Derive macro expects a struct")
                        .to_compile_error();
                }
            }

            let name = &input.ident;
            let expanded = quote! {
                impl #name {
                    #(#functions)*
                }

                impl crate::settings::SettingGroup for #name {
                    fn register(&self) {
                        let s: Self = Default::default();
                        crate::settings::SETTINGS.set(&s);
                        #(#register_fragments)*
                    }
                }
            };
            println!("{}", expanded.clone().to_string());
            TokenStream::from(expanded)
        }
        None => syn::Error::new_spanned(
            input.ident,
            "Expected name value attribute #[setting_prefix = \"my_prefix\"]",
        )
        .to_compile_error()
        .into(),
    }
}

fn setting_prefix(attrs: Vec<syn::Attribute>) -> Option<String> {
    for attr in attrs.iter() {
        if let Ok(Meta::NameValue(name_value)) = attr.parse_meta() {
            if name_value.path.is_ident("setting_prefix") {
                if let syn::Lit::Str(literal) = name_value.lit {
                    return Some(literal.value());
                }
            }
        }
    }
    None
}
