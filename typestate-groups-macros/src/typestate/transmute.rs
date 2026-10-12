//! The `TransmutableState` impl `unsafe_transmute = true` derives.

use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::{
    Generics, Ident, Type, WherePredicate, parse_quote,
    parse_quote_spanned, spanned::Spanned as _,
};

use super::{TypeState, args::Alignment, fields::FieldShape};

/// `TransmutableState<S2>` for `Struct<S>`, for every `S2` with a
/// matching layout.
pub(super) struct TransmutableStateImpl<'a> {
    pub(super) typestate: &'a TypeState,
    pub(super) align: &'a Alignment,
}

impl ToTokens for TransmutableStateImpl<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let TypeState {
            item_struct,
            target_state,
            ..
        } = self.typestate;
        let struct_ident = &item_struct.ident;
        let target_state = &target_state.ident;
        let (_, ty_generics, _) = item_struct.generics.split_for_impl();
        let generics = self.generics();
        let (impl_generics, _, where_clause) = generics.split_for_impl();
        let layout_check = LayoutCheck {
            typestate: self.typestate,
        };

        tokens.extend(quote! {
            // SAFETY: the where-clause gives every projection one size
            // in both states and every indirect field one layout, and
            // the `repr` fixes the field order.
            // `LAYOUT_CHECK` rejects any alignment or field offset that
            // `align = N` lets differ, and a pointer that turns fat.
            unsafe impl #impl_generics ::typestate_groups::TransmutableState<#target_state>
                for #struct_ident #ty_generics #where_clause
            {
                #layout_check
            }
        });
    }
}

impl TransmutableStateImpl<'_> {
    /// The target generics, bounded so that every projection and every
    /// indirect field keeps its layout.
    fn generics(&self) -> Generics {
        let typestate = self.typestate;
        let mut generics = typestate.target_generics.clone();
        let predicates = &mut generics.make_where_clause().predicates;
        predicates.extend(
            typestate
                .projections()
                .map(|projection| self.layout_predicate(projection)),
        );
        predicates.extend(typestate.indirect_fields().map(
            |src| -> WherePredicate {
                let dst = typestate.in_target_state(src);
                parse_quote_spanned! {src.span()=>
                    #dst: ::typestate_groups::Repointed<#src>
                }
            },
        ));
        generics
    }

    /// `S2::__TypestateGroupsLayoutP:
    /// SameLayout<S::__TypestateGroupsLayoutP>`, or only `SameSize`
    /// under `align = N`.
    fn layout_predicate(&self, projection: &Ident) -> WherePredicate {
        let state = &self.typestate.state;
        let target_state = &self.typestate.target_state.ident;
        let layout = crate::naming::layout_assoc_ident(projection);

        match self.align {
            Alignment::Inferred => parse_quote! {
                #target_state::#layout: ::typestate_groups::SameLayout<#state::#layout>
            },
            Alignment::Forced(_) => parse_quote! {
                <#target_state::#layout as ::typestate_groups::TypeLayout>::Size:
                    ::typestate_groups::SameSize<<#state::#layout as ::typestate_groups::TypeLayout>::Size>
            },
        }
    }
}

impl TypeState {
    /// The associated types the fields hold by value (`Value` in a field
    /// of type `S::Value`).
    fn projections(&self) -> impl Iterator<Item = &Ident> {
        self.shapes.iter().filter_map(|shape| match shape {
            FieldShape::Projection(assoc) => Some(assoc),
            FieldShape::Fixed | FieldShape::Indirect(_) => None,
        })
    }

    /// The types of the fields that hold the state's types behind a
    /// pointer.
    fn indirect_fields(&self) -> impl Iterator<Item = &Type> {
        self.shapes.iter().filter_map(|shape| match shape {
            FieldShape::Indirect(ty) => Some(&**ty),
            FieldShape::Fixed | FieldShape::Projection(_) => None,
        })
    }
}

/// `TransmutableState::LAYOUT_CHECK`, extended with one offset
/// assertion per field.
///
/// `align = N` pins the container's alignment but not its fields', so
/// a field after a projection can sit at another offset in the
/// target state.
struct LayoutCheck<'a> {
    typestate: &'a TypeState,
}

impl ToTokens for LayoutCheck<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let TypeState {
            item_struct,
            target_ty,
            ..
        } = self.typestate;
        let struct_ident = &item_struct.ident;
        let align_msg = format!(
            "raise `align = N` on `{struct_ident}` to at least the \
             largest alignment among its states"
        );
        let offset_asserts = item_struct.fields.members().map(|member| {
            let msg = format!(
                "move field `{}` to the start of `{struct_ident}`, or \
                 transmute only between states whose types share an \
                 alignment: its offset differs in the target state",
                quote!(#member),
            );
            quote! {
                ::core::assert!(
                    ::core::mem::offset_of!(Self, #member)
                        == ::core::mem::offset_of!(#target_ty, #member),
                    #msg
                );
            }
        });

        tokens.extend(quote! {
            const LAYOUT_CHECK: () = {
                ::core::assert!(
                    ::core::mem::align_of::<Self>()
                        == ::core::mem::align_of::<#target_ty>(),
                    #align_msg
                );
                #(#offset_asserts)*
                ::core::assert!(
                    ::core::mem::size_of::<Self>()
                        == ::core::mem::size_of::<#target_ty>(),
                    "`Self` and `Target` must have the same size"
                );
            };
        });
    }
}
