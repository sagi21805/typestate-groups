//! The `TransmutableState` impl `unsafe_transmute = true` derives.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Ident, Type, WherePredicate, parse_quote, parse_quote_spanned,
    spanned::Spanned as _,
};

use super::{TypeState, args::Alignment, fields::FieldShape};

impl TypeState {
    /// `TransmutableState<S2>` for `Struct<S>`, for every `S2` with a
    /// matching layout.
    pub(super) fn transmutable_state_impl(
        &self,
        align: &Alignment,
    ) -> TokenStream {
        let struct_ident = &self.item_struct.ident;
        let target_state = &self.target_state.ident;
        let (_, ty_generics, _) =
            self.item_struct.generics.split_for_impl();

        let mut generics = self.target_generics.clone();
        let predicates = &mut generics.make_where_clause().predicates;
        predicates.extend(
            self.projections().map(|projection| {
                self.layout_predicate(projection, align)
            }),
        );
        predicates.extend(self.indirect_fields().map(
            |src| -> WherePredicate {
                let dst = self.in_target_state(src);
                parse_quote_spanned! {src.span()=>
                    #dst: ::typestate_groups::Repointed<#src>
                }
            },
        ));
        let (impl_generics, _, where_clause) = generics.split_for_impl();
        let layout_check = self.layout_check();

        quote! {
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
        }
    }

    /// The associated types the fields hold by value (`Value` in a field
    /// of type `S::Value`).
    pub(super) fn projections(&self) -> impl Iterator<Item = &Ident> {
        self.shapes.iter().filter_map(|shape| match shape {
            FieldShape::Projection(assoc) => Some(assoc),
            FieldShape::Fixed
            | FieldShape::SelfPointer(_)
            | FieldShape::Indirect(_) => None,
        })
    }

    /// The types of the fields that hold the state's types, or the
    /// struct itself, behind a pointer.
    fn indirect_fields(&self) -> impl Iterator<Item = &Type> {
        self.shapes.iter().filter_map(|shape| match shape {
            FieldShape::SelfPointer(ty) | FieldShape::Indirect(ty) => {
                Some(&**ty)
            }
            FieldShape::Fixed | FieldShape::Projection(_) => None,
        })
    }

    /// `TransmutableState::LAYOUT_CHECK`, extended with one offset
    /// assertion per field.
    ///
    /// `align = N` pins the container's alignment but not its fields', so
    /// a field after a projection can sit at another offset in the
    /// target state.
    fn layout_check(&self) -> TokenStream {
        let struct_ident = &self.item_struct.ident;
        let target_ty = &self.target_ty;
        let align_msg = format!(
            "raise `align = N` on `{struct_ident}` to at least the \
             largest alignment among its states"
        );
        let offset_asserts = self.item_struct.fields.members().map(|member| {
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

        quote! {
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
        }
    }

    /// `S2::__TypestateGroupsLayoutP:
    /// SameLayout<S::__TypestateGroupsLayoutP>`, or only `SameSize`
    /// under `align = N`.
    fn layout_predicate(
        &self,
        projection: &Ident,
        align: &Alignment,
    ) -> WherePredicate {
        let state = &self.state;
        let target_state = &self.target_state.ident;
        let layout = crate::naming::layout_assoc_ident(projection);

        match align {
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
