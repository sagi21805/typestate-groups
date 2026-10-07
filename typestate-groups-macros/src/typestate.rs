//! `#[typestate]`: `WithState` and `Restate` for every typestate, plus
//! `TransmutableState`, `CastableState` and `Permits` under
//! `unsafe_transmute = true`.

mod args;
mod cast;
mod fields;
mod item_struct;
mod transmute;

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Generics, Ident, ItemStruct, Type, TypeParam, parse_quote};

pub(crate) use self::args::TypeStateArgs;
use self::{
    args::Transmute,
    fields::{FieldExt as _, FieldShape},
    item_struct::ItemStructExt as _,
};
use crate::syn_ext::TypeExt as _;

pub(crate) struct TypeState {
    /// The struct with the state bounded by `State`.
    item_struct: ItemStruct,
    /// The generic type parameter carrying the state.
    state: Ident,
    /// The target state, with the state's inline bounds.
    target_state: TypeParam,
    /// `Wrap<S> -> Wrap<__TypestateGroupsTargetState>`
    target_ty: TokenStream,
    /// The generics every derived impl starts from: the struct's, plus
    /// the target state.
    target_generics: Generics,
    /// Whether `TransmutableState`, `CastableState` and `Permits` are
    /// derived too.
    transmute: Transmute,
    /// How each field changes between states, in field order. Empty
    /// without `unsafe_transmute = true`.
    shapes: Vec<FieldShape>,
}

impl TypeState {
    /// Resolves the state and validates the fields.
    pub(crate) fn new(
        args: TypeStateArgs,
        mut item_struct: ItemStruct,
    ) -> syn::Result<Self> {
        let state_param = item_struct.state_param(args.state.as_ref())?;
        state_param
            .bounds
            .push(parse_quote!(::typestate_groups::State));

        let target_state = TypeParam {
            ident: crate::naming::target_state_ident(),
            ..state_param.clone()
        };
        let state = state_param.ident.clone();
        let target_ty =
            item_struct.with_state(&state, &target_state.ident);
        let target_generics =
            item_struct.target_generics(&state, &target_state);
        item_struct.require_no_marker(&state)?;

        let shapes = match &args.transmute {
            Transmute::On(align) => {
                let shapes: Vec<FieldShape> = item_struct
                    .fields
                    .iter()
                    .map(|field| field.shape(&state, &item_struct.ident))
                    .collect();
                if shapes
                    .iter()
                    .all(|shape| matches!(shape, FieldShape::Fixed))
                {
                    return Err(syn::Error::new_spanned(
                        &item_struct.ident,
                        format!(
                            "add a field of type `{state}::Assoc` or a \
                             pointer to one, or remove `unsafe_transmute \
                             = true`",
                        ),
                    ));
                }
                if shapes.iter().any(|shape| {
                    matches!(shape, FieldShape::SelfPointer(_))
                }) {
                    if let Some(FieldShape::Indirect(pointer)) =
                        shapes.iter().find(|shape| {
                            matches!(shape, FieldShape::Indirect(_))
                        })
                    {
                        return Err(syn::Error::new_spanned(
                            pointer,
                            format!(
                                "make this field `{state}::Assoc` or a \
                                 type without `{state}`, or convert with \
                                 `morph`: `{}` points at itself, so its \
                                 other fields can't point at the state's \
                                 types",
                                item_struct.ident,
                            ),
                        ));
                    }
                }
                item_struct.ensure_repr(align)?;
                shapes
            }
            Transmute::Off => Vec::new(),
        };

        Ok(TypeState {
            item_struct,
            state,
            target_state,
            target_ty,
            target_generics,
            transmute: args.transmute,
            shapes,
        })
    }

    /// The struct followed by every impl `#[typestate]` derives for it.
    pub(crate) fn generate_typestate_impls(&self) -> TokenStream {
        let item_struct = &self.item_struct;
        let with_state_impl = self.with_state_impl();
        let restate_impl = self.restate_impl();
        let transmute_impls = match &self.transmute {
            Transmute::On(align) => {
                let transmutable_state_impl =
                    self.transmutable_state_impl(align);
                let castable_state_impls = self.castable_state_impls();
                let permits_impls = self.permits_impls();
                Some(quote! {
                    #transmutable_state_impl

                    #castable_state_impls

                    #permits_impls
                })
            }
            Transmute::Off => None,
        };

        quote! {
            #item_struct

            #with_state_impl

            #restate_impl

            #transmute_impls
        }
    }

    fn with_state_impl(&self) -> TokenStream {
        let struct_ident = &self.item_struct.ident;
        let state = &self.state;
        let (impl_generics, ty_generics, where_clause) =
            self.item_struct.generics.split_for_impl();

        quote! {
            impl #impl_generics ::typestate_groups::WithState for #struct_ident #ty_generics #where_clause {
                type State = #state;
            }
        }
    }

    /// `Restate<S2>` for `Struct<S>`, for every `S2`.
    fn restate_impl(&self) -> TokenStream {
        let struct_ident = &self.item_struct.ident;
        let target_state = &self.target_state.ident;
        let target_ty = &self.target_ty;
        let (_, ty_generics, _) =
            self.item_struct.generics.split_for_impl();
        let (impl_generics, _, where_clause) =
            self.target_generics.split_for_impl();

        quote! {
            impl #impl_generics ::typestate_groups::Restate<#target_state>
                for #struct_ident #ty_generics #where_clause
            {
                type Target = #target_ty;
            }
        }
    }

    /// `ty` in the target state.
    ///
    /// `NonNull<S::Value>` -> `NonNull<S2::Value>`
    fn in_target_state(&self, ty: &Type) -> Type {
        ty.renamed(&self.state, &self.target_state.ident)
    }
}
