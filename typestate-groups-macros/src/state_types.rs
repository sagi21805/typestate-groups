use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::{ItemTrait, TraitItem, TraitItemType, parse_quote};

pub struct StateTypes<'ast> {
    inner: &'ast ItemTrait,
    /// The trait's associated types, possibly none.
    types: Vec<&'ast TraitItemType>,
}

impl<'ast> StateTypes<'ast> {
    /// Reads the trait's associated types. A trait without any tags its
    /// states into groups that set no types.
    pub fn new(inner: &'ast ItemTrait) -> StateTypes<'ast> {
        let types = inner
            .items
            .iter()
            .filter_map(|item| match item {
                TraitItem::Type(ty) => Some(ty),
                _ => None,
            })
            .collect();

        StateTypes { inner, types }
    }
}

impl ToTokens for StateTypes<'_> {
    /// The trait plus its `{Trait}GroupMarker` and `{Trait}GroupMember`
    /// traits.
    ///
    /// ```ignore
    /// trait Meta { type A; type B; }
    /// // ->
    /// trait Meta: ::typestate_groups::State {
    ///     type A;
    ///     type B;
    ///     type __TypestateGroupsLayoutA;
    ///     type __TypestateGroupsLayoutB;
    ///     type Marker: MetaGroupMarker<A = Self::A, B = Self::B>;
    /// }
    /// trait MetaGroupMarker { type A; type B; }
    /// trait MetaGroupMember<G: MetaGroupMarker>:
    ///     Meta<Marker = G, A = G::A, B = G::B> {}
    /// ```
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let vis = &self.inner.vis;
        let trait_ident = &self.inner.ident;
        let marker_name = crate::naming::group_marker_ident(trait_ident);
        let member_name = crate::naming::group_member_ident(trait_ident);
        let group = crate::naming::group_param_ident();
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

        let member_bound = quote! {
            #trait_ident<Marker = #group, #(#assocs = #group::#assocs),*>
        };

        tokens.extend(quote! {
            #original

            #vis trait #marker_name {
                #(#types)*
            }

            #vis trait #member_name<#group: #marker_name>: #member_bound {}

            impl<#group: #marker_name, T: #member_bound> #member_name<#group> for T {}
        });
    }
}
