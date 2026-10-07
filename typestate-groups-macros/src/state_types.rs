use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    GenericArgument, ItemTrait, TraitItem, TraitItemType, parse_quote,
};

use crate::syn_ext::GenericsExt as _;

pub struct StateTypes<'ast> {
    inner: &'ast ItemTrait,
    /// The trait's associated types, one or more.
    types: Vec<&'ast TraitItemType>,
}

impl<'ast> StateTypes<'ast> {
    pub fn new(inner: &'ast ItemTrait) -> syn::Result<StateTypes<'ast>> {
        let types: Vec<&TraitItemType> = inner
            .items
            .iter()
            .filter_map(|item| match item {
                TraitItem::Type(ty) => Some(ty),
                _ => None,
            })
            .collect();

        if types.is_empty() {
            return Err(syn::Error::new_spanned(
                &inner.ident,
                format!(
                    "add an associated type to `{}`, such as `type \
                     Value;`, for its groups to set",
                    inner.ident
                ),
            ));
        }

        Ok(StateTypes { inner, types })
    }

    /// The trait plus its `{Trait}GroupMarker` and `{Trait}GroupMember`
    /// traits. The member trait takes the trait's parameters after the
    /// group.
    ///
    /// ```ignore
    /// trait Meta<T> { type A; type B; }
    /// // ->
    /// trait Meta<T>: ::typestate_groups::State {
    ///     type A;
    ///     type B;
    ///     type __TypestateGroupsLayoutA;
    ///     type __TypestateGroupsLayoutB;
    ///     type Marker: MetaGroupMarker<A = Self::A, B = Self::B>;
    /// }
    /// trait MetaGroupMarker { type A; type B; }
    /// trait MetaGroupMember<G: MetaGroupMarker, T>:
    ///     Meta<T, Marker = G, A = G::A, B = G::B> {}
    /// ```
    pub fn create_group_marker(&self) -> syn::Result<TokenStream> {
        let vis = &self.inner.vis;
        let trait_ident = &self.inner.ident;
        let marker_name = crate::naming::group_marker_ident(trait_ident);
        let member_name = crate::naming::group_member_ident(trait_ident);
        let group = crate::naming::group_param_ident();
        let implementor = crate::naming::implementor_ident();
        let types = &self.types;
        let assocs: Vec<_> = types.iter().map(|ty| &ty.ident).collect();

        let mut original = self.inner.clone();
        original
            .items
            .extend(assocs.iter().map(|assoc| -> TraitItem {
                let layout = crate::naming::layout_assoc_ident(assoc);
                parse_quote! {
                    #[doc(hidden)]
                    type #layout: ::typestate_groups::TypeLayout;
                }
            }));
        original.items.push(parse_quote! {
            type Marker: #marker_name<#(#assocs = Self::#assocs),*>;
        });
        original
            .supertraits
            .push(parse_quote!(::typestate_groups::State));

        let generics = &self.inner.generics;
        let mut trait_args = generics.to_arguments();
        trait_args.push(parse_quote!(Marker = #group));
        trait_args.extend(assocs.iter().map(|assoc| -> GenericArgument {
            parse_quote!(#assoc = #group::#assoc)
        }));
        let member_bound = quote!(#trait_ident<#trait_args>);

        let mut member_generics = generics.clone();
        member_generics.params.insert(
            generics.lifetimes().count(),
            parse_quote!(#group: #marker_name),
        );
        let (_, member_args, _) = member_generics.split_for_impl();
        let mut blanket_generics = member_generics.clone();
        blanket_generics
            .params
            .push(parse_quote!(#implementor: #member_bound));
        let (blanket_impl_generics, _, _) =
            blanket_generics.split_for_impl();
        let member_where_clause = &generics.where_clause;

        Ok(quote! {
            #original

            #vis trait #marker_name {
                #(#types)*
            }

            #vis trait #member_name #member_generics: #member_bound #member_where_clause {}

            impl #blanket_impl_generics #member_name #member_args for #implementor #member_where_clause {}
        })
    }
}
