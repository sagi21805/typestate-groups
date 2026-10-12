//! The `CastableState` impls `unsafe_transmute = true` derives.

use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::{
    Generics, WherePredicate, parse_quote, parse_quote_spanned,
    spanned::Spanned as _,
};

use super::{TypeState, fields::FieldShape};

/// `CastableState<S2, B>` for `Struct<S>`, for one `CastBy` `B` and
/// every `S2` whose projections and pointees stay valid under it.
///
/// `value: S::Value` through `ByRef` ->
/// `ReadShared: Permits<S::Value, S2::Value>`
pub(super) struct CastableStateImpl<'a> {
    pub(super) typestate: &'a TypeState,
    pub(super) by: CastBy,
}

impl ToTokens for CastableStateImpl<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let TypeState {
            item_struct,
            target_state,
            ..
        } = self.typestate;
        let struct_ident = &item_struct.ident;
        let target_state = &target_state.ident;
        let by = self.by;
        let (_, ty_generics, _) = item_struct.generics.split_for_impl();
        let generics = self.generics();
        let (impl_generics, _, where_clause) = generics.split_for_impl();
        let pointee_check = PointeeCheck {
            typestate: self.typestate,
        };

        tokens.extend(quote! {
            // SAFETY: the where-clause proves every projection and
            // pointee valid in the target state under the access
            // this `CastBy` gives it, `POINTEE_CHECK` keeps every
            // pointee's size and alignment, and every other field
            // keeps its type.
            unsafe impl #impl_generics ::typestate_groups::CastableState<#target_state, #by>
                for #struct_ident #ty_generics #where_clause
            {
                #pointee_check
            }
        });
    }
}

impl CastableStateImpl<'_> {
    /// The target generics, bounded so that the struct transmutes to the
    /// target state and every field stays valid in it.
    fn generics(&self) -> Generics {
        let TypeState {
            item_struct,
            target_state,
            target_generics,
            shapes,
            ..
        } = self.typestate;
        let struct_ident = &item_struct.ident;
        let target_state = &target_state.ident;
        let (_, ty_generics, _) = item_struct.generics.split_for_impl();

        let mut generics = target_generics.clone();
        let predicates = &mut generics.make_where_clause().predicates;
        predicates.push(parse_quote! {
            #struct_ident #ty_generics:
                ::typestate_groups::TransmutableState<#target_state>
        });
        predicates.extend(
            shapes.iter().flat_map(|shape| self.predicates(shape)),
        );
        generics
    }

    /// The bounds that keep a field valid in the target state when the
    /// container is cast `by`.
    ///
    /// `S::P -> ReadShared: Permits<S::P, S2::P>`,
    /// `F<S> -> F<S>: Indirect, F<S2>: Indirect,
    /// <F<S> as Indirect>::CastStateRef:
    /// Permits<<F<S> as Indirect>::Pointee, <F<S2> as Indirect>::Pointee>`
    fn predicates(&self, shape: &FieldShape) -> Vec<WherePredicate> {
        let state = &self.typestate.state;
        let target_state = &self.typestate.target_state.ident;

        match shape {
            FieldShape::Fixed => Vec::new(),
            FieldShape::Projection(assoc) => {
                let access = self.by.field_access();
                vec![parse_quote! {
                    #access: ::typestate_groups::Permits<#state::#assoc, #target_state::#assoc>
                }]
            }
            FieldShape::Indirect(src) => {
                let dst = self.typestate.in_target_state(src);
                let access = self.by.pointee_access();
                vec![
                    parse_quote_spanned! {src.span()=>
                        #src: ::typestate_groups::Indirect
                    },
                    parse_quote_spanned! {src.span()=>
                        #dst: ::typestate_groups::Indirect
                    },
                    parse_quote_spanned! {src.span()=>
                        <#src as ::typestate_groups::Indirect>::#access:
                            ::typestate_groups::Permits<
                                <#src as ::typestate_groups::Indirect>::Pointee,
                                <#dst as ::typestate_groups::Indirect>::Pointee,
                            >
                    },
                ]
            }
        }
    }
}

/// `CastableState::POINTEE_CHECK`, asserting that each indirect
/// field's pointee keeps its size and alignment.
struct PointeeCheck<'a> {
    typestate: &'a TypeState,
}

impl ToTokens for PointeeCheck<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let TypeState {
            item_struct,
            shapes,
            ..
        } = self.typestate;
        let asserts = item_struct
            .fields
            .members()
            .zip(shapes)
            .filter_map(|(member, shape)| match shape {
                FieldShape::Indirect(src) => {
                    let dst = self.typestate.in_target_state(src);
                    let msg = format!(
                        "make the pointee of `{}` keep its size and \
                         alignment in the target state, or convert with \
                         `morph`",
                        quote!(#member),
                    );
                    Some(quote! {
                        ::core::assert!(
                            ::core::mem::size_of::<<#src as ::typestate_groups::Indirect>::Pointee>()
                                == ::core::mem::size_of::<<#dst as ::typestate_groups::Indirect>::Pointee>()
                                && ::core::mem::align_of::<<#src as ::typestate_groups::Indirect>::Pointee>()
                                    == ::core::mem::align_of::<<#dst as ::typestate_groups::Indirect>::Pointee>(),
                            #msg
                        );
                    })
                }
                FieldShape::Fixed | FieldShape::Projection(_) => None,
            });

        tokens.extend(quote! {
            const POINTEE_CHECK: () = {
                #(#asserts)*
            };
        });
    }
}

/// How a cast holds the container, one per `typestate_groups::CastBy`
/// type.
#[derive(Clone, Copy)]
pub(super) enum CastBy {
    /// By value, with `cast_state`.
    Value,
    /// Through `&`, with `cast_state_ref`.
    Ref,
    /// Through `&mut`, with `cast_state_mut`.
    Mut,
}

impl CastBy {
    pub(super) const ALL: [CastBy; 3] =
        [CastBy::Value, CastBy::Ref, CastBy::Mut];

    /// The `typestate_groups::Access` a field held by value gets.
    ///
    /// `Ref -> ::typestate_groups::ReadShared`
    fn field_access(self) -> TokenStream {
        match self {
            CastBy::Value => quote!(::typestate_groups::Read),
            CastBy::Ref => quote!(::typestate_groups::ReadShared),
            CastBy::Mut => quote!(::typestate_groups::ReadWrite),
        }
    }

    /// The `Indirect` associated type naming a pointee's access.
    ///
    /// `Ref -> CastStateRef`
    fn pointee_access(self) -> TokenStream {
        match self {
            CastBy::Value => quote!(CastState),
            CastBy::Ref => quote!(CastStateRef),
            CastBy::Mut => quote!(CastStateMut),
        }
    }
}

impl ToTokens for CastBy {
    /// `CastBy::Ref` -> `::typestate_groups::ByRef`
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.extend(match self {
            CastBy::Value => quote!(::typestate_groups::ByValue),
            CastBy::Ref => quote!(::typestate_groups::ByRef),
            CastBy::Mut => quote!(::typestate_groups::ByMut),
        });
    }
}
