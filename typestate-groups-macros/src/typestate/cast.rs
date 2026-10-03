//! The `CastableState` impls `unsafe_transmute = true` derives.

use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::{
    WherePredicate, parse_quote, parse_quote_spanned,
    spanned::Spanned as _,
};

use super::{TypeState, fields::FieldShape};

impl TypeState {
    /// `CastableState<S2, R>` for `Struct<S>`, for every `CastReceiver`
    /// `R` and every `S2` whose projections and pointees stay valid
    /// under it.
    pub(super) fn castable_state_impls(&self) -> TokenStream {
        let struct_ident = &self.item_struct.ident;
        let target_state = &self.target_state.ident;
        let (_, ty_generics, _) =
            self.item_struct.generics.split_for_impl();

        CastReceiver::ALL
            .into_iter()
            .map(|receiver| {
                let mut generics = self.target_generics.clone();
                let predicates = &mut generics.make_where_clause().predicates;
                predicates.push(parse_quote! {
                    #struct_ident #ty_generics:
                        ::typestate_groups::TransmutableState<#target_state>
                });
                predicates.extend(
                    self.shapes
                        .iter()
                        .filter_map(|shape| self.cast_predicate(shape, receiver)),
                );
                let (impl_generics, _, where_clause) =
                    generics.split_for_impl();
                let pointee_check = self.pointee_check(receiver);

                quote! {
                    // SAFETY: the where-clause proves every projection and
                    // pointee valid in the target state under this receiver,
                    // `POINTEE_CHECK` keeps every pointee's size and
                    // alignment, and every other field keeps its type.
                    unsafe impl #impl_generics ::typestate_groups::CastableState<#target_state, #receiver>
                        for #struct_ident #ty_generics #where_clause
                    {
                        #pointee_check
                    }
                }
            })
            .collect()
    }

    /// The bound that keeps a field valid in the target state when the
    /// container is cast through `receiver`.
    ///
    /// `S::P -> S2::P: CastValid<S::P, R>`,
    /// `F<S> -> F<S2>: CastIndirect<F<S>, R>`
    fn cast_predicate(
        &self,
        shape: &FieldShape,
        receiver: CastReceiver,
    ) -> Option<WherePredicate> {
        let state = &self.state;
        let target_state = &self.target_state.ident;

        match shape {
            FieldShape::Fixed => None,
            FieldShape::Projection(assoc) => Some(parse_quote! {
                #target_state::#assoc:
                    ::typestate_groups::CastValid<#state::#assoc, #receiver>
            }),
            FieldShape::Indirect(src) => {
                let dst = self.in_target_state(src);
                Some(parse_quote_spanned! {src.span()=>
                    #dst: ::typestate_groups::CastIndirect<#src, #receiver>
                })
            }
        }
    }

    /// `CastableState::POINTEE_CHECK` for `receiver`, asserting each
    /// indirect field's `CastIndirect::SAME_POINTEE_LAYOUT`.
    fn pointee_check(&self, receiver: CastReceiver) -> TokenStream {
        let asserts = self
            .item_struct
            .fields
            .members()
            .zip(&self.shapes)
            .filter_map(|(member, shape)| match shape {
                FieldShape::Indirect(src) => {
                    let dst = self.in_target_state(src);
                    let msg = format!(
                        "make the pointee of `{}` keep its size and \
                         alignment in the target state, or convert with \
                         `morph`",
                        quote!(#member),
                    );
                    Some(quote! {
                        ::core::assert!(
                            <#dst as ::typestate_groups::CastIndirect<#src, #receiver>>::SAME_POINTEE_LAYOUT,
                            #msg
                        );
                    })
                }
                FieldShape::Fixed | FieldShape::Projection(_) => None,
            });

        quote! {
            const POINTEE_CHECK: () = {
                #(#asserts)*
            };
        }
    }
}

/// How a cast holds the container, one per
/// `typestate_groups::CastReceiver` type.
#[derive(Clone, Copy)]
enum CastReceiver {
    /// By value, with `cast_state`.
    Value,
    /// Through `&`, with `cast_state_ref`.
    Ref,
    /// Through `&mut`, with `cast_state_mut`.
    Mut,
}

impl CastReceiver {
    const ALL: [CastReceiver; 3] =
        [CastReceiver::Value, CastReceiver::Ref, CastReceiver::Mut];
}

impl ToTokens for CastReceiver {
    /// `CastReceiver::Ref` -> `::typestate_groups::ByRef`
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.extend(match self {
            CastReceiver::Value => quote!(::typestate_groups::ByValue),
            CastReceiver::Ref => quote!(::typestate_groups::ByRef),
            CastReceiver::Mut => {
                quote!(::typestate_groups::ByMut)
            }
        });
    }
}
