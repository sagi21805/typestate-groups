use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::ItemStruct;

pub(crate) struct State<'ast> {
    item_struct: &'ast ItemStruct,
}

impl<'ast> State<'ast> {
    pub(crate) fn new(item_struct: &'ast ItemStruct) -> State<'ast> {
        State { item_struct }
    }
}

impl ToTokens for State<'_> {
    /// The struct followed by its `State` impl.
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let item_struct = self.item_struct;
        let struct_ident = &item_struct.ident;
        let (impl_generics, ty_generics, where_clause) =
            item_struct.generics.split_for_impl();

        tokens.extend(quote! {
            #item_struct

            impl #impl_generics ::typestate_groups::State for #struct_ident #ty_generics #where_clause {}
        });
    }
}
