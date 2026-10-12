use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::{Ident, ImplItem, ImplItemType, ItemImpl, LitInt, Type};

pub struct Group<'ast> {
    inner_impl: &'ast ItemImpl,
    group_name: &'ast Ident,
}

impl<'ast> Group<'ast> {
    pub fn new(
        item_impl: &'ast ItemImpl,
        group_name: &'ast Ident,
    ) -> Group<'ast> {
        Group {
            inner_impl: item_impl,
            group_name,
        }
    }

    pub fn generate_group_impl(&self) -> syn::Result<TokenStream> {
        let trait_name = self.trait_name()?;
        let states = self.states()?;

        let items = &self.inner_impl.items;
        let layouts = self.layouts()?;
        let size_asserts = layouts.iter().map(|layout| &layout.assert);
        let layout_items = layouts.iter().map(|layout| &layout.item);

        let stripped_items =
            items.iter().map(|item| StrippedItem { item });
        let items_tokens = quote! { #(#stripped_items)* };
        let layout_tokens = quote! { #(#layout_items)* };
        let types = items
            .iter()
            .filter(|item| matches!(item, ImplItem::Type(_)))
            .map(|item| StrippedItem { item });
        let marker_trait = crate::naming::group_marker_ident(trait_name);
        let group_name = self.group_name;

        Ok(quote! {
            pub struct #group_name;

            impl ::typestate_groups::Group for #group_name {}

            #(#size_asserts)*

            impl #marker_trait for #group_name {
                #(#types)*
            }

            #(
                impl #trait_name for #states {
                    #items_tokens
                    type Marker = #group_name;
                    #layout_tokens
                }
            )*
        })
    }

    /// `impl Testing for (StateA, StateB)` -> `Testing`
    fn trait_name(&self) -> syn::Result<&'ast Ident> {
        let (trait_path, _) =
            self.inner_impl.trait_.as_ref().ok_or_else(|| {
                syn::Error::new_spanned(
                    self.inner_impl,
                    "use `#[group]` on a trait impl, such as `impl Meta \
                     for (StateA, StateB)`",
                )
            })?;

        trait_path.get_ident().ok_or_else(|| {
            syn::Error::new_spanned(
                trait_path,
                "name the trait with a single identifier, and bring it \
                 into scope with `use`",
            )
        })
    }

    /// `(StateA, StateB) -> [StateA, StateB]`
    fn states(&self) -> syn::Result<Vec<&'ast Ident>> {
        let Type::Tuple(tup) = self.inner_impl.self_ty.as_ref() else {
            return Err(syn::Error::new_spanned(
                &self.inner_impl.self_ty,
                "implement the trait for a tuple of states, such as \
                 `(StateA, StateB)`",
            ));
        };

        tup.elems
            .iter()
            .map(|e| {
                match e {
                    Type::Path(p) => p.path.get_ident(),
                    _ => None,
                }
                .ok_or_else(|| {
                    syn::Error::new_spanned(
                        e,
                        "name each state with a single identifier, such \
                         as `(StateA, StateB)`",
                    )
                })
            })
            .collect()
    }

    /// The layout of every associated type.
    fn layouts(&self) -> syn::Result<Vec<TypeLayout<'ast>>> {
        self.inner_impl
            .items
            .iter()
            .filter_map(|item| match item {
                ImplItem::Type(impl_ty) => {
                    Some(TypeLayout::new(impl_ty, self.group_name))
                }
                _ => None,
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
struct TypeLayout<'a> {
    item: TokenStream,
    assert: Option<SizeAssert<'a>>,
}

impl<'a> TypeLayout<'a> {
    fn new(impl_ty: &'a ImplItemType, group: &Ident) -> syn::Result<Self> {
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

        let assert = SizeAssert::try_from(impl_ty)?;
        let size = &assert.size;
        let ty = &impl_ty.ty;
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

/// `const _: () = assert!(size_of::<u32>() == 4, ..);`, read from
/// `#[size(4)] type Value = u32;`.
struct SizeAssert<'a> {
    impl_ty: &'a ImplItemType,
    size: LitInt,
}

impl<'a> TryFrom<&'a ImplItemType> for SizeAssert<'a> {
    type Error = syn::Error;

    /// Reads `#[size(N)]`, rejecting other attributes.
    fn try_from(impl_ty: &'a ImplItemType) -> syn::Result<Self> {
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

        Ok(SizeAssert {
            impl_ty,
            size: attr.parse_args()?,
        })
    }
}

impl ToTokens for SizeAssert<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let SizeAssert {
            impl_ty: ImplItemType { ident, ty, .. },
            size,
        } = self;
        let msg = format!(
            "change `#[size({size})]` on `{ident}` to the size of `{}`",
            quote!(#ty),
        );

        tokens.extend(quote! {
            const _: () = assert!(::core::mem::size_of::<#ty>() == #size, #msg);
        });
    }
}

/// An impl item with `#[size(N)]` removed from associated types.
struct StrippedItem<'a> {
    item: &'a ImplItem,
}

impl ToTokens for StrippedItem<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self.item {
            ImplItem::Type(impl_ty) => ImplItemType {
                attrs: Vec::new(),
                ..impl_ty.clone()
            }
            .to_tokens(tokens),
            item => item.to_tokens(tokens),
        }
    }
}
