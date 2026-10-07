//! The `CastableState` and `Permits` impls `unsafe_transmute = true`
//! derives.

use core::iter;

use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::{
    Ident, WherePredicate, parse_quote, parse_quote_spanned,
    spanned::Spanned as _,
};

use super::{TypeState, fields::FieldShape};

impl TypeState {
    /// `CastableState<S2, B>` for `Struct<S>`, for every `CastBy` `B`
    /// and every `S2` whose projections and pointees stay valid under
    /// it.
    ///
    /// `value: S::Value` through `ByRef` ->
    /// `ReadShared: Permits<S::Value, S2::Value>`
    pub(super) fn castable_state_impls(&self) -> TokenStream {
        let struct_ident = &self.item_struct.ident;
        let target_state = &self.target_state.ident;
        let (_, ty_generics, _) =
            self.item_struct.generics.split_for_impl();
        let pointee_check = self.pointee_check();

        CastBy::ALL
            .into_iter()
            .map(|by| {
                let mut generics = self.target_generics.clone();
                let predicates = &mut generics.make_where_clause().predicates;
                predicates.push(parse_quote! {
                    #struct_ident #ty_generics:
                        ::typestate_groups::TransmutableState<#target_state>
                });
                predicates.extend(
                    self.shapes
                        .iter()
                        .flat_map(|shape| self.cast_predicates(shape, by)),
                );
                let (impl_generics, _, where_clause) =
                    generics.split_for_impl();

                quote! {
                    // SAFETY: the where-clause proves every projection and
                    // pointee valid in the target state under the access
                    // this `CastBy` gives it, `POINTEE_CHECK` keeps every
                    // pointee's size and alignment, and every other field
                    // keeps its type. A pointer to the struct itself
                    // reaches a struct this same claim covers
                    // (`SameType`), with every projection checked under
                    // that pointer's access too. Accesses compose by
                    // sharing if either side shares and by needing both
                    // ways only if both do, so every node reachable along
                    // a chain of such pointers is covered.
                    unsafe impl #impl_generics ::typestate_groups::CastableState<#target_state, #by>
                        for #struct_ident #ty_generics #where_clause
                    {
                        #pointee_check
                    }
                }
            })
            .collect()
    }

    /// The bounds that keep a field valid in the target state when the
    /// container is cast `by`.
    ///
    /// `S::P -> ReadShared: Permits<S::P, S2::P>`,
    /// `F<Node<S>> -> <F<Node<S>> as Indirect>::Pointee:
    /// SameType<Node<S>>,
    /// <F<Node<S>> as Indirect>::CastStateRef: Permits<S::P, S2::P>`,
    /// `F<S> -> F<S>: Indirect, F<S2>: Indirect,
    /// <F<S> as Indirect>::CastStateRef:
    /// Permits<<F<S> as Indirect>::Pointee, <F<S2> as Indirect>::Pointee>`
    fn cast_predicates(
        &self,
        shape: &FieldShape,
        by: CastBy,
    ) -> Vec<WherePredicate> {
        match shape {
            FieldShape::Fixed => Vec::new(),
            FieldShape::Projection(assoc) => {
                vec![self.permits_projection(by.field_access(), assoc)]
            }
            FieldShape::SelfPointer(src) => {
                let struct_ident = &self.item_struct.ident;
                let (_, ty_generics, _) =
                    self.item_struct.generics.split_for_impl();
                let access = by.pointee_access();
                let pointee_access = quote! {
                    <#src as ::typestate_groups::Indirect>::#access
                };

                iter::once(parse_quote_spanned! {src.span()=>
                    <#src as ::typestate_groups::Indirect>::Pointee:
                        ::typestate_groups::SameType<#struct_ident #ty_generics>
                })
                .chain(self.projections().map(|assoc| {
                    self.permits_projection(&pointee_access, assoc)
                }))
                .collect()
            }
            FieldShape::Indirect(src) => {
                let dst = self.in_target_state(src);
                let access = by.pointee_access();
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

    /// `access: Permits<S::assoc, S2::assoc>`
    fn permits_projection(
        &self,
        access: impl ToTokens,
        assoc: &Ident,
    ) -> WherePredicate {
        let state = &self.state;
        let target_state = &self.target_state.ident;

        parse_quote! {
            #access: ::typestate_groups::Permits<#state::#assoc, #target_state::#assoc>
        }
    }

    /// `Permits<Struct<S>, Struct<S2>>` for every `Access`, bounded by the
    /// casts that prove it, so another container can point at this one.
    ///
    /// `ReadWrite` -> `where Struct<S>: CastableState<S2, ByMut>`
    pub(super) fn permits_impls(&self) -> TokenStream {
        let struct_ident = &self.item_struct.ident;
        let target_state = &self.target_state.ident;
        let target_ty = &self.target_ty;
        let (_, ty_generics, _) =
            self.item_struct.generics.split_for_impl();

        Access::ALL
            .into_iter()
            .map(|access| {
                let mut generics = self.target_generics.clone();
                generics.make_where_clause().predicates.extend(
                    access.casts().iter().map(|by| -> WherePredicate {
                        parse_quote! {
                            #struct_ident #ty_generics:
                                ::typestate_groups::CastableState<#target_state, #by>
                        }
                    }),
                );
                let (impl_generics, _, where_clause) =
                    generics.split_for_impl();

                quote! {
                    // SAFETY: the `CastableState` bounds prove every valid
                    // source container a valid target of the same size, by
                    // value for `Read`, through `&` for `ReadShared`,
                    // through `&mut` for `ReadWrite`, and through both for
                    // `ReadWriteShared`.
                    unsafe impl #impl_generics ::typestate_groups::Permits<
                        #struct_ident #ty_generics, #target_ty
                    > for #access #where_clause {}
                }
            })
            .collect()
    }

    /// `CastableState::POINTEE_CHECK`, asserting that each indirect
    /// field's pointee keeps its size and alignment.
    fn pointee_check(&self) -> TokenStream {
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
                            ::core::mem::size_of::<<#src as ::typestate_groups::Indirect>::Pointee>()
                                == ::core::mem::size_of::<<#dst as ::typestate_groups::Indirect>::Pointee>()
                                && ::core::mem::align_of::<<#src as ::typestate_groups::Indirect>::Pointee>()
                                    == ::core::mem::align_of::<<#dst as ::typestate_groups::Indirect>::Pointee>(),
                            #msg
                        );
                    })
                }
                FieldShape::Fixed
                | FieldShape::Projection(_)
                | FieldShape::SelfPointer(_) => None,
            });

        quote! {
            const POINTEE_CHECK: () = {
                #(#asserts)*
            };
        }
    }
}

/// How a cast holds the container, one per `typestate_groups::CastBy`
/// type.
#[derive(Clone, Copy)]
enum CastBy {
    /// By value, with `cast_state`.
    Value,
    /// Through `&`, with `cast_state_ref`.
    Ref,
    /// Through `&mut`, with `cast_state_mut`.
    Mut,
}

impl CastBy {
    const ALL: [CastBy; 3] = [CastBy::Value, CastBy::Ref, CastBy::Mut];

    /// The access a field held by value gets.
    ///
    /// `Ref -> ReadShared`
    fn field_access(self) -> Access {
        match self {
            CastBy::Value => Access::Read,
            CastBy::Ref => Access::ReadShared,
            CastBy::Mut => Access::ReadWrite,
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

/// How cast bytes are used, one per `typestate_groups::Access` type.
#[derive(Clone, Copy)]
enum Access {
    Read,
    ReadShared,
    ReadWrite,
    ReadWriteShared,
}

impl Access {
    const ALL: [Access; 4] = [
        Access::Read,
        Access::ReadShared,
        Access::ReadWrite,
        Access::ReadWriteShared,
    ];

    /// The casts that prove a container's bytes valid under this access.
    ///
    /// `ReadWriteShared -> [Mut, Ref]`
    fn casts(self) -> &'static [CastBy] {
        match self {
            Access::Read => &[CastBy::Value],
            Access::ReadShared => &[CastBy::Ref],
            Access::ReadWrite => &[CastBy::Mut],
            Access::ReadWriteShared => &[CastBy::Mut, CastBy::Ref],
        }
    }
}

impl ToTokens for Access {
    /// `Access::ReadShared` -> `::typestate_groups::ReadShared`
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.extend(match self {
            Access::Read => quote!(::typestate_groups::Read),
            Access::ReadShared => quote!(::typestate_groups::ReadShared),
            Access::ReadWrite => quote!(::typestate_groups::ReadWrite),
            Access::ReadWriteShared => {
                quote!(::typestate_groups::ReadWriteShared)
            }
        });
    }
}
