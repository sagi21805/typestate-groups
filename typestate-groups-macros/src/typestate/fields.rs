//! Where the state appears in the struct's field types.

use extend::ext;
use syn::{
    Field, Ident, Path, PathArguments, Type, TypePath, TypeReference,
    WherePredicate, parse_quote,
    visit::{self, Visit},
};

use crate::syn_ext::TypeExt as _;

/// How a field changes between states, for transmuting.
pub(super) enum FieldShape {
    /// The same type in every state: no `S`, or a ZST such as
    /// `PhantomData<S::Value>`.
    Fixed,
    /// `S::Assoc`, laid out by its group's `#[size(N)]`.
    Projection(Ident),
    /// A pointer to the struct itself, such as
    /// `Option<NonNull<Node<S>>>`.
    SelfPointer(Box<Type>),
    /// Any other type, which must implement `typestate_groups::Indirect`.
    /// This cover mostly pointer types like Box<T>, NonNull<T>, &'a mut T,
    /// etc.
    Indirect(Box<Type>),
}

#[ext]
pub(super) impl Field {
    /// How this field of `container` changes between states.
    ///
    /// `S::Value -> Projection(Value)`,
    /// `Option<NonNull<Node<S>>> -> SelfPointer(..)`,
    /// `Option<NonNull<S::Value>> -> Indirect(..)`
    fn shape(&self, state: &Ident, container: &Ident) -> FieldShape {
        let ty = &self.ty;
        if !ty.mentions_ident(state) || ty.is_zst() {
            FieldShape::Fixed
        } else if let Some(assoc) = ty.state_projection(state) {
            FieldShape::Projection(assoc.clone())
        } else if ty.mentions_ident(container) {
            // TODO: An optimization could be storing the &self.ty
            // reference here, if ownership is not really needed.
            FieldShape::SelfPointer(Box::new(ty.clone()))
        } else {
            FieldShape::Indirect(Box::new(ty.clone()))
        }
    }
}

#[ext]
pub(super) impl Type {
    /// The first `S::{assoc}` at any depth of this type.
    ///
    /// `Option<S::Marker>`, `"Marker"` -> `Some(Marker)`
    fn find_projection(
        &self,
        state: &Ident,
        assoc: &str,
    ) -> Option<&Ident> {
        let mut finder = FindProjection {
            state,
            assoc,
            found: None,
        };
        finder.visit_type(self);
        finder.found
    }

    /// `T: 'a` for every `&'a T` inside this type whose `T` mentions
    /// `state`.
    ///
    /// `Option<&'a S::Value>` -> `[S::Value: 'a]`
    fn outlives(&self, state: &Ident) -> Vec<WherePredicate> {
        let mut outlives = Outlives {
            state,
            found: Vec::new(),
        };
        outlives.visit_type(self);
        outlives.found
    }

    /// `Assoc` when this type is `S::Assoc, or (S::Assoc)`.
    fn state_projection(&self, state: &Ident) -> Option<&Ident> {
        let Type::Path(TypePath {
            qself: None,
            path:
                Path {
                    leading_colon: None,
                    segments,
                },
            ..
        }) = self.peeled()
        else {
            return None;
        };

        match segments.iter().collect::<Vec<_>>().as_slice() {
            [state_seg, type_seg] => (state_seg.ident == *state
                && state_seg.arguments == PathArguments::None
                && type_seg.arguments == PathArguments::None)
                .then_some(&type_seg.ident),
            _ => None,
        }
    }
}

/// Finds the first `S::{assoc}` inside one type, and stops visiting once
/// it has.
///
/// Run once per field type through `Type::find_projection`.
/// `ItemStruct::require_no_marker` runs it over the fields until one
/// holds `S::Marker`.
struct FindProjection<'a, 'ast> {
    state: &'a Ident,
    assoc: &'a str,
    found: Option<&'ast Ident>,
}

impl<'ast> Visit<'ast> for FindProjection<'_, 'ast> {
    fn visit_type(&mut self, ty: &'ast Type) {
        if self.found.is_some() {
            return;
        }
        match ty.state_projection(self.state) {
            Some(assoc) if assoc == self.assoc => self.found = Some(assoc),
            Some(_) => {}
            None => visit::visit_type(self, ty),
        }
    }
}

/// Collects `T: 'a` for every `&'a T` inside one type whose `T` mentions
/// the state.
///
/// Run once per field type through `Type::outlives`.
/// `ItemStruct::target_generics` runs it over every field. A field
/// `&'a S::Value` implies `S::Value: 'a` for `Struct<S>`, but rustc
/// doesn't carry that over to `S2`, so `Struct<S2>` would be ill-formed
/// in the derived impls without `S2::Value: 'a`.
struct Outlives<'a> {
    state: &'a Ident,
    found: Vec<WherePredicate>,
}

impl<'ast> Visit<'ast> for Outlives<'_> {
    fn visit_type_reference(&mut self, reference: &'ast TypeReference) {
        match reference {
            TypeReference {
                lifetime: Some(lifetime),
                elem,
                ..
            } if elem.mentions_ident(self.state) => {
                self.found.push(parse_quote!(#elem: #lifetime));
            }
            _ => {}
        }
        visit::visit_type_reference(self, reference);
    }
}
