//! Reading and adjusting the struct `#[typestate]` is applied to.

use extend::ext;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Attribute, ConstParam, GenericParam, Generics, Ident, ItemStruct,
    LifetimeParam, TypeParam, WherePredicate, parse_quote,
};

use super::{args::Alignment, fields::TypeExt as _};
use crate::syn_ext::{
    AttributeExt as _, GenericsExt as _, WherePredicateExt as _,
};

#[ext]
pub(super) impl ItemStruct {
    /// The type parameter named `state`, or the only one.
    fn state_param(
        &mut self,
        state: Option<&Ident>,
    ) -> syn::Result<&mut TypeParam> {
        match state {
            Some(state) => {
                let msg = format!(
                    "set `state` to one of the generic type parameters \
                     of `{}`",
                    self.ident
                );
                self.generics
                    .type_param_mut(state)
                    .ok_or_else(|| syn::Error::new(state.span(), msg))
            }
            None => self.infer_state_param(),
        }
    }

    /// The struct's only generic type parameter.
    fn infer_state_param(&mut self) -> syn::Result<&mut TypeParam> {
        let mut type_params = self.generics.type_params_mut();

        type_params
            .next()
            .filter(|_| type_params.next().is_none())
            .ok_or_else(|| {
                syn::Error::new_spanned(
                    &self.ident,
                    "add `state = <Ident>` to `#[typestate]` to name the \
                     state parameter; only a struct with one generic \
                     type parameter can leave it out",
                )
            })
    }

    /// Rejects a field that mentions `S::Marker` at any depth.
    fn require_no_marker(&self, state: &Ident) -> syn::Result<()> {
        let marker = self
            .fields
            .iter()
            .find_map(|field| field.ty.find_projection(state, "Marker"));

        match marker {
            Some(marker) => Err(syn::Error::new(
                marker.span(),
                format!(
                    "replace `{state}::Marker` with \
                     `PhantomData<{state}>`: a group marker has no value \
                     to convert"
                ),
            )),
            None => Ok(()),
        }
    }

    /// Adds `#[repr(C)]` if missing, and `#[repr(align(N))]` when forced.
    fn ensure_repr(&mut self, align: &Alignment) -> syn::Result<()> {
        let reprs: Vec<&Attribute> = self
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("repr"))
            .collect();

        match reprs.as_slice() {
            [] => self.attrs.push(parse_quote!(#[repr(C)])),
            [first, ..]
                if !reprs.iter().any(|attr| attr.guarantees_layout()) =>
            {
                return Err(syn::Error::new_spanned(
                    first,
                    "add `C` to this `#[repr(..)]`: `unsafe_transmute = \
                     true` needs a guaranteed field layout",
                ));
            }
            _ => {}
        }

        if let Alignment::Forced(n) = align {
            self.attrs.push(parse_quote!(#[repr(align(#n))]));
        }

        Ok(())
    }

    /// The struct's generics plus `target_state`, which gets a copy of
    /// every `where` predicate on `state` and of every outlives bound a
    /// reference field implies.
    ///
    /// `where S: Debug` -> `where S: Debug, S2: Debug`,
    /// `field: &'a S::Value` -> `where S2::Value: 'a`
    fn target_generics(
        &self,
        state: &Ident,
        target_state: &TypeParam,
    ) -> Generics {
        let mut generics = self.generics.clone();
        let implied = self
            .fields
            .iter()
            .flat_map(|field| field.ty.outlives(state));
        let target_predicates: Vec<WherePredicate> = generics
            .where_clause
            .iter()
            .flat_map(|clause| clause.predicates.iter().cloned())
            .chain(implied)
            .filter_map(|predicate| {
                predicate.renamed(state, &target_state.ident)
            })
            .collect();

        generics
            .params
            .push(GenericParam::Type(target_state.clone()));
        generics
            .make_where_clause()
            .predicates
            .extend(target_predicates);
        generics
    }

    /// `Wrap<S> -> Wrap<target_state>`
    fn with_state(
        &self,
        state: &Ident,
        target_state: &Ident,
    ) -> TokenStream {
        let struct_ident = &self.ident;
        let args = self.generics.params.iter().map(|param| match param {
            GenericParam::Type(param) if param.ident == *state => {
                quote!(#target_state)
            }
            GenericParam::Type(TypeParam { ident, .. })
            | GenericParam::Const(ConstParam { ident, .. }) => {
                quote!(#ident)
            }
            GenericParam::Lifetime(LifetimeParam { lifetime, .. }) => {
                quote!(#lifetime)
            }
        });

        quote!(#struct_ident<#(#args),*>)
    }
}
