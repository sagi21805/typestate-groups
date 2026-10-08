use extend::ext;
use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::{
    AngleBracketedGenericArguments, GenericArgument, GenericParam,
    Generics, Ident, ImplItem, ImplItemType, ItemImpl, LifetimeParam,
    LitInt, Path, Token, Type, TypeParam,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

use crate::syn_ext::{
    GenericArgumentExt as _, GenericParamExt as _, PathExt as _,
    TypeExt as _, WherePredicateExt as _,
};

pub struct Group<'ast> {
    inner_impl: &'ast ItemImpl,
    args: &'ast GroupArgs,
    /// The trait, such as `List`.
    trait_path: &'ast Path,
    states: Vec<&'ast Type>,
    /// The impl's parameters that the group lists, in the group's order.
    params: Vec<&'ast GenericParam>,
}

impl<'ast> Group<'ast> {
    pub fn new(
        item_impl: &'ast ItemImpl,
        args: &'ast GroupArgs,
    ) -> syn::Result<Group<'ast>> {
        Ok(Group {
            trait_path: item_impl.trait_path()?,
            states: item_impl.states()?,
            params: args.impl_params(&item_impl.generics)?,
            inner_impl: item_impl,
            args,
        })
    }

    /// The group struct, its marker impl, and the trait impl for every
    /// state.
    ///
    /// ```ignore
    /// #[group(FreeList<T>)]
    /// impl<T: Slab> List for (FreeHead<T>, FreeTail<T>) { type Head = FreeHead<T>; }
    /// // ->
    /// pub struct FreeList<T>(PhantomData<fn() -> (T,)>);
    /// impl<T> Group for FreeList<T> {}
    /// impl<T: Slab> ListGroupMarker for FreeList<T> { type Head = FreeHead<T>; }
    /// impl<T: Slab> List for FreeHead<T> {
    ///     type Head = FreeHead<T>;
    ///     type Marker = FreeList<T>;
    ///     type __TypestateGroupsLayoutHead = UnpinnedTypeLayout<FreeList<T>, FreeHead<T>>;
    /// }
    /// // and `impl<T: Slab> List for FreeTail<T>` likewise
    /// ```
    pub fn generate_group_impl(&self) -> syn::Result<TokenStream> {
        let mut items = self.inner_impl.items.clone();
        self.require_group_level_types(&items)?;
        let layouts = self.layouts(&mut items)?;

        let group_struct = self.group_struct();
        let size_asserts = layouts.iter().map(|layout| &layout.assert);
        let marker_impl = self.marker_impl(&items);
        let state_impls = self.state_impls(&items, &layouts);

        Ok(quote! {
            #group_struct

            #(#size_asserts)*

            #marker_impl

            #state_impls
        })
    }

    /// The marker trait impl for the group, holding the associated
    /// types.
    ///
    /// `type Head = FreeHead<T>;` -> `impl<T: Slab> ListGroupMarker for
    /// FreeList<T> { type Head = FreeHead<T>; }`
    fn marker_impl(&self, items: &[ImplItem]) -> TokenStream {
        let marker_trait = self
            .trait_path
            .with_last_ident(crate::naming::group_marker_ident);
        let group = self.args;
        let generics = self.marker_generics();
        let (impl_generics, _, where_clause) = generics.split_for_impl();
        let types = items
            .iter()
            .filter(|item| matches!(item, ImplItem::Type(_)));

        quote! {
            impl #impl_generics #marker_trait for #group #where_clause {
                #(#types)*
            }
        }
    }

    /// The trait impl for every state, with the group as its `Marker`
    /// and the layout of every associated type.
    fn state_impls(
        &self,
        items: &[ImplItem],
        layouts: &[TypeLayout],
    ) -> TokenStream {
        let trait_path = self.trait_path;
        let states = &self.states;
        let group = self.args;
        let items_tokens = quote! { #(#items)* };
        let layout_items = layouts.iter().map(|layout| &layout.item);
        let layout_tokens = quote! { #(#layout_items)* };
        let (impl_generics, _, where_clause) =
            self.inner_impl.generics.split_for_impl();

        quote! {
            #(
                impl #impl_generics #trait_path for #states #where_clause {
                    #items_tokens
                    type Marker = #group;
                    #layout_tokens
                }
            )*
        }
    }

    /// The names of the impl's parameters that the group doesn't list.
    fn other_params(&self) -> Vec<&'ast Ident> {
        self.inner_impl
            .generics
            .params
            .iter()
            .map(|param| param.name())
            .filter(|name| {
                !self.params.iter().any(|param| param.name() == *name)
            })
            .collect()
    }

    /// Rejects an associated type that mentions an impl parameter the
    /// group doesn't list, which the marker impl couldn't name.
    fn require_group_level_types(
        &self,
        items: &[ImplItem],
    ) -> syn::Result<()> {
        let name = &self.args.name;
        let others = self.other_params();

        for item in items {
            let ImplItem::Type(ImplItemType { ident, ty, .. }) = item
            else {
                continue;
            };
            if let Some(param) =
                others.iter().find(|param| ty.mentions_ident(param))
            {
                return Err(syn::Error::new_spanned(
                    ty,
                    format!(
                        "list `{param}` in the group, as in \
                         `#[group({name}<{param}>)]`: `{ident}` is \
                         shared by the whole group"
                    ),
                ));
            }
        }
        Ok(())
    }

    /// The layout of every associated type, stripping `#[size(N)]`.
    fn layouts(
        &self,
        items: &mut [ImplItem],
    ) -> syn::Result<Vec<TypeLayout>> {
        items
            .iter_mut()
            .filter_map(|item| match item {
                ImplItem::Type(impl_ty) => {
                    Some(TypeLayout::new(impl_ty, self.args, &self.params))
                }
                _ => None,
            })
            .collect()
    }

    /// The group struct, holding its parameters in a `PhantomData`, and
    /// its `Group` impl.
    ///
    /// `FreeList<'a, T>` -> `pub struct FreeList<'a, T>(PhantomData<fn()
    /// -> (&'a (), T)>);`
    fn group_struct(&self) -> TokenStream {
        let name = &self.args.name;
        let generics = Generics {
            params: self
                .params
                .iter()
                .map(|param| param.unbounded())
                .collect(),
            ..Generics::default()
        };
        let (impl_generics, ty_generics, _) = generics.split_for_impl();
        let phantoms =
            self.params.iter().filter_map(|param| match param {
                GenericParam::Lifetime(LifetimeParam {
                    lifetime, ..
                }) => Some(quote!(&#lifetime ())),
                GenericParam::Type(TypeParam { ident, .. }) => {
                    Some(quote!(#ident))
                }
                GenericParam::Const(_) => None,
            });

        let fields = (!self.params.is_empty()).then(|| {
            quote! {
                (::core::marker::PhantomData<fn() -> (#(#phantoms,)*)>)
            }
        });

        quote! {
            pub struct #name #impl_generics #fields;

            impl #impl_generics ::typestate_groups::Group for #name #ty_generics {}
        }
    }

    /// The group's parameters with their bounds, and the impl's
    /// where-predicates that mention no other parameter.
    fn marker_generics(&self) -> Generics {
        let impl_generics = &self.inner_impl.generics;
        let others = self.other_params();

        let mut generics = Generics {
            params: self
                .params
                .iter()
                .map(|&param| param.clone())
                .collect(),
            where_clause: None,
            ..impl_generics.clone()
        };
        let predicates = impl_generics
            .where_clause
            .iter()
            .flat_map(|clause| &clause.predicates)
            .filter(|predicate| {
                !others.iter().any(|other| predicate.mentions_ident(other))
            })
            .cloned();
        generics.make_where_clause().predicates.extend(predicates);
        generics
    }
}

/// `#[group(Name)]` or `#[group(Name<T, 'a, N>)]`, which lists the impl
/// parameters the group carries.
pub struct GroupArgs {
    name: Ident,
    params: Punctuated<GenericArgument, Token![,]>,
}

impl GroupArgs {
    /// The parameters of `generics` that the group lists, in the
    /// group's order.
    ///
    /// `#[group(FreeList<T>)] impl<'a, T: Slab>` -> `[T: Slab]`
    fn impl_params<'g>(
        &self,
        generics: &'g Generics,
    ) -> syn::Result<Vec<&'g GenericParam>> {
        self.params
            .iter()
            .map(|arg| {
                let param = arg.param_name().and_then(|name| {
                    generics
                        .params
                        .iter()
                        .find(|param| param.name() == name)
                });
                param.ok_or_else(|| {
                    syn::Error::new_spanned(
                        arg,
                        "list a parameter of this impl by its name, such \
                         as `#[group(FreeList<T>)]` for `impl<T>`",
                    )
                })
            })
            .collect()
    }
}

impl Parse for GroupArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name = input.parse()?;
        let params = if input.peek(Token![<]) {
            input.parse::<AngleBracketedGenericArguments>()?.args
        } else {
            Punctuated::new()
        };

        Ok(GroupArgs { name, params })
    }
}

impl ToTokens for GroupArgs {
    /// `FreeList<T>`
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let GroupArgs { name, params } = self;
        tokens.extend(if params.is_empty() {
            quote!(#name)
        } else {
            quote!(#name<#params>)
        });
    }
}

#[ext]
impl ItemImpl {
    /// `impl Testing<T> for (StateA, StateB)` -> `Testing<T>`
    fn trait_path(&self) -> syn::Result<&Path> {
        self.trait_
            .as_ref()
            .map(|(trait_path, _)| trait_path)
            .ok_or_else(|| {
                syn::Error::new_spanned(
                    self,
                    "use `#[group]` on a trait impl, such as `impl Meta \
                     for (StateA, StateB)`",
                )
            })
    }

    /// `(StateA, StateB<T>) -> [StateA, StateB<T>]`
    fn states(&self) -> syn::Result<Vec<&Type>> {
        let Type::Tuple(tup) = self.self_ty.as_ref() else {
            return Err(syn::Error::new_spanned(
                &self.self_ty,
                "implement the trait for a tuple of states, such as \
                 `(StateA, StateB)`",
            ));
        };

        tup.elems
            .iter()
            .map(|state| match state.peeled() {
                Type::Path(_) => Ok(state),
                _ => Err(syn::Error::new_spanned(
                    state,
                    "name each state with a type path, such as `(StateA, \
                     StateB<T>)`",
                )),
            })
            .collect()
    }
}

/// An associated type's layout item and size assertion.
///
/// ```ignore
/// #[size(4)] type Value = u32;
/// // ->
/// const _: () = assert!(size_of::<u32>() == 4, ..);
/// type __TypestateGroupsLayoutValue = PinnedTypeLayout<G, u32, 4, { align_of::<u32>() }>;
///
/// type Value = String;
/// // ->
/// type __TypestateGroupsLayoutValue = UnpinnedTypeLayout<G, String>;
/// ```
struct TypeLayout {
    item: TokenStream,
    assert: Option<TokenStream>,
}

impl TypeLayout {
    fn new(
        impl_ty: &mut ImplItemType,
        group: &GroupArgs,
        group_params: &[&GenericParam],
    ) -> syn::Result<Self> {
        let layout = crate::naming::layout_assoc_ident(&impl_ty.ident);

        if impl_ty.attrs.is_empty() {
            let ty = &impl_ty.ty;
            return Ok(TypeLayout {
                item: quote! {
                    type #layout = ::typestate_groups::UnpinnedTypeLayout<#group, #ty>;
                },
                assert: None,
            });
        }

        let assoc = impl_ty.ident.clone();
        let SizeAssert { assert, size, ty } =
            SizeAssert::try_from(impl_ty)?;
        if let Some(param) = group_params
            .iter()
            .map(|param| param.name())
            .find(|param| ty.mentions_ident(param))
        {
            return Err(syn::Error::new_spanned(
                ty,
                format!(
                    "remove `#[size({size})]` from `{assoc}`, and \
                     convert with `morph`: its layout depends on \
                     `{param}`"
                ),
            ));
        }
        Ok(TypeLayout {
            item: quote! {
                type #layout = ::typestate_groups::PinnedTypeLayout<
                    #group, #ty, #size, { ::core::mem::align_of::<#ty>() }
                >;
            },
            assert: Some(assert),
        })
    }
}

/// A compile-time assertion read from `#[size(N)]`.
pub struct SizeAssert<'a> {
    pub assert: TokenStream,
    pub size: LitInt,
    pub ty: &'a Type,
}

impl<'a> TryFrom<&'a mut ImplItemType> for SizeAssert<'a> {
    type Error = syn::Error;

    /// Reads and removes `#[size(N)]`, rejecting other attributes.
    fn try_from(impl_ty: &'a mut ImplItemType) -> syn::Result<Self> {
        let ident = &impl_ty.ident;

        let [attr] = impl_ty.attrs.as_slice() else {
            return Err(syn::Error::new(
                ident.span(),
                format!(
                    "keep one attribute, `#[size(N)]`, on associated \
                     type `{ident}` inside `#[group]`"
                ),
            ));
        };

        if !attr.path().is_ident("size") {
            return Err(syn::Error::new_spanned(
                attr,
                format!(
                    "remove this attribute: `#[group]` allows only \
                     `#[size(N)]` on associated type `{ident}`"
                ),
            ));
        }

        let size: LitInt = attr.parse_args()?;

        let ty = &impl_ty.ty;
        let msg = format!(
            "change `#[size({size})]` on `{ident}` to the size of `{}`",
            quote!(#ty),
        );

        impl_ty.attrs.clear();

        Ok(SizeAssert {
            assert: quote! {
                const _: () = assert!(::core::mem::size_of::<#ty>() == #size, #msg);
            },
            size,
            ty,
        })
    }
}
