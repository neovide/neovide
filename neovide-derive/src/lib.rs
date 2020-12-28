use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Meta};

#[proc_macro_derive(SettingGroup, attributes(setting_prefix))]
pub fn setting_group(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let prefix = setting_prefix(input.attrs.as_ref())
        .map(|p| format!("_{}", p))
        .unwrap_or("".to_string());
    prepare_tokens(input, prefix)
}

fn prepare_tokens(input: DeriveInput, prefix: String) -> TokenStream {
    const ERR_MSG: &'static str = "Derive macro expects a struct";
    match input.data {
        syn::Data::Struct(ref data) => tokens_for_struct(input.ident, prefix, data),
        syn::Data::Enum(data) => syn::Error::new_spanned(data.enum_token, ERR_MSG)
            .to_compile_error()
            .into(),
        syn::Data::Union(data) => syn::Error::new_spanned(data.union_token, ERR_MSG)
            .to_compile_error()
            .into(),
    }
}

fn tokens_for_struct(name: syn::Ident, prefix: String, data: &syn::DataStruct) -> TokenStream {
    // TODO: get rid of this caching
    let mut functions = vec![];
    let mut register_fragments = vec![];
    for field in data.fields.iter() {
        if let Some(ref ident) = field.ident {
            let update_func_name = quote::format_ident!("update_{}", ident);
            let reader_func_name = quote::format_ident!("reader_{}", ident);
            functions.push(quote! {
                // TODO: operate on self
                fn #update_func_name (value: rmpv::Value) {
                    println!("Update");
                    let mut s = crate::settings::SETTINGS.get::<Self>();
                    s.#ident.from_value(value);
                    crate::settings::SETTINGS.set(&s);
                }

                fn #reader_func_name () -> rmpv::Value {
                    println!("Read");
                    let s = crate::settings::SETTINGS.get::<Self>();
                    s.#ident.into()
                }
            });
            let vim_setting_name = format!("{}_{}", prefix, ident);
            register_fragments.push(quote! {
                crate::settings::SETTINGS.set_setting_handlers(
                    #vim_setting_name,
                    Self::#update_func_name,
                    Self::#reader_func_name
                );
            })
        } else {
            syn::Error::new_spanned(field.colon_token, "Expected named struct fields")
                // issue
                .to_compile_error();
        }
    }
    let expanded = quote! {
        impl #name {
            #(#functions)*
        }

        impl crate::settings::SettingGroup for #name {
            fn register(&self) {
                println!("Registered");
                crate::settings::SETTINGS.set(self);
                #(#register_fragments)*
            }
        }
    };
    // println!("{}", expanded.to_string());
    TokenStream::from(expanded)
}

fn setting_prefix(attrs: &[syn::Attribute]) -> Option<String> {
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
