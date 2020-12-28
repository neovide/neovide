use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Meta};

#[proc_macro_derive(SettingGroup, attributes(setting_prefix))]
pub fn setting_group(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    match setting_prefix(input.attrs) {
        Some(prefix) => {
            let mut functions = vec![];
            let mut register_fragments = vec![];
            match input.data {
                syn::Data::Struct(data) => {
                    for field in data.fields.into_iter() {
                        if let Some(ident) = field.ident {
                            let type_name = field.ty;
                            let update_func_name = quote::format_ident!("update_{}", ident);
                            let reader_func_name = quote::format_ident!("reader_{}", ident);
                            functions.push(quote! {
                                fn #update_func_name (value: Value) {
                                    let mut s = crate::settings::SETTINGS.get::<#type_name>();
                                    s.#ident.from_value(value);
                                    SETTINGS.set(&s);
                                }

                                fn #reader_func_name () -> Value {
                                    let s = crate::settings::SETTINGS.get::<#type_name>();
                                    s.#ident.into()
                                }
                            });
                            let vim_setting_name = quote::format_ident!("{}_{}", prefix, ident);
                            register_fragments.push(quote! {
                                crate::settings::SETTINGS.set_setting_handlers(
                                    #vim_setting_name,
                                    #update_func_name,
                                    #reader_func_name
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
                impl neovide::settings::SettingBlock for #name {
                    #(#functions)*

                    fn register(&self) {
                        #(#register_fragments)*
                    }
                }
            };
            println!("{:?}", expanded.clone());
            TokenStream::from(expanded)
        }
        None => {
            syn::Error::new_spanned(
                input.ident,
                "Expected name value attribute #[setting_prefix=\"my_prefex\"]",
            )
            .to_compile_error();
            TokenStream::new()
        }
    }
}

fn setting_prefix(attrs: Vec<syn::Attribute>) -> Option<String> {
    for attr in attrs.iter() {
        if attr.clone().style == syn::AttrStyle::Outer {
            if let Ok(Meta::NameValue(name_value)) = attr.parse_meta() {
                if name_value.path.is_ident("setting_prefix") {
                    if let syn::Lit::Str(literal) = name_value.lit {
                        return Some(literal.value());
                    }
                }
            }
        }
    }
    None
}
