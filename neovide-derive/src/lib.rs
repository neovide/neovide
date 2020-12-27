use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn setting_block(attr: TokenStream, item: TokenStream) -> TokenStream {
    println!("attr: \"{}\"", attr.to_string());
    println!("item: \"{}\"", item.to_string());
    item
}
