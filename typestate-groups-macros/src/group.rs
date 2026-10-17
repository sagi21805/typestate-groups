use extend::ext;
use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::{
    Ident, ImplItem, ImplItemType, ItemImpl, LitInt, Type, parse_quote,
};

pub struct Group<'ast> {
    group_name: &'ast Ident,
    /// `Meta` in `impl Meta for (StateA, StateB)`.
    trait_name: &'ast Ident,
    states: Vec<&'ast Ident>,
    items: &'ast [ImplItem],
    layouts: Vec<TypeLayout<'ast>>,
}

impl<'ast> Group<'ast> {
    /// Reads the trait, the states and the layout of every associated
    /// type.
    pub fn new(
        item_impl: &'ast ItemImpl,
        group_name: &'ast Ident,
    ) -> syn::Result<Group<'ast>> {
        Ok(Group {
            group_name,
            trait_name: item_impl.trait_name()?,
            states: item_impl.states()?,
            items: &item_impl.items,
            layouts: item_impl.layouts(group_name)?,
        })
    }
}

impl ToTokens for Group<'_> {
    /// The group's marker type, its size assertions, and the trait impl
    /// for every state.
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Group {
            group_name,
            trait_name,
            states,
            items,
            layouts,
        } = self;
        let size_asserts =
            layouts.iter().map(|layout| &layout.size_assert);

        let stripped_items =
            items.iter().map(|item| StrippedItem { item });
        let items_tokens = quote! { #(#stripped_items)* };
        let layout_tokens = quote! { #(#layouts)* };
        let types = items
            .iter()
            .filter(|item| matches!(item, ImplItem::Type(_)))
            .map(|item| StrippedItem { item });
        let marker_trait = crate::naming::group_marker_ident(trait_name);

        tokens.extend(quote! {
            pub struct #group_name;

            impl ::typestate_groups::Group for #group_name {}

            #(#size_asserts)*

            impl #marker_trait for #group_name {
                #(#types)*
                #layout_tokens
            }

            #(
                impl #trait_name for #states {
                    #items_tokens
                    type Marker = #group_name;
                    #layout_tokens
                }
            )*
        });
    }
}

#[ext]
impl ItemImpl {
    /// `impl Testing for (StateA, StateB)` -> `Testing`
    fn trait_name(&self) -> syn::Result<&Ident> {
        let (trait_path, _) = self.trait_.as_ref().ok_or_else(|| {
            syn::Error::new_spanned(
                self,
                "use `#[group]` on a trait impl, such as `impl Meta for \
                 (StateA, StateB)`",
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
    fn states(&self) -> syn::Result<Vec<&Ident>> {
        let Type::Tuple(tup) = self.self_ty.as_ref() else {
            return Err(syn::Error::new_spanned(
                &self.self_ty,
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
    fn layouts<'a>(
        &'a self,
        group: &'a Ident,
    ) -> syn::Result<Vec<TypeLayout<'a>>> {
        self.items
            .iter()
            .filter_map(|item| match item {
                ImplItem::Type(impl_ty) => {
                    Some(TypeLayout::new(impl_ty, group))
                }
                _ => None,
            })
            .collect()
    }
}

/// An associated type's layout item.
///
/// ```ignore
/// #[size(4)] type Value = u32;
/// // ->
/// type __TypestateGroupsLayoutValue = PinnedTypeLayout<G, u32, 4, { align_of::<u32>() }>;
///
/// type Value = String;
/// // ->
/// type __TypestateGroupsLayoutValue = UnpinnedTypeLayout<G, String>;
/// ```
struct TypeLayout<'a> {
    impl_ty: &'a ImplItemType,
    group: &'a Ident,
    /// The assertion `#[size(N)]` adds, if present.
    size_assert: Option<SizeAssert<'a>>,
}

impl<'a> TypeLayout<'a> {
    /// Reads `#[size(N)]` when the type has attributes.
    fn new(
        impl_ty: &'a ImplItemType,
        group: &'a Ident,
    ) -> syn::Result<Self> {
        let size_assert = if impl_ty.attrs.is_empty() {
            None
        } else {
            Some(SizeAssert::try_from(impl_ty)?)
        };

        Ok(TypeLayout {
            impl_ty,
            group,
            size_assert,
        })
    }
}

impl ToTokens for TypeLayout<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let TypeLayout {
            impl_ty: ImplItemType { ident, ty, .. },
            group,
            size_assert,
        } = self;
        let layout = crate::naming::layout_assoc_ident(ident);
        let layout_ty: Type = match size_assert {
            None => parse_quote! {
                ::typestate_groups::UnpinnedTypeLayout<#group, #ty>
            },
            Some(SizeAssert { size, .. }) => parse_quote! {
                ::typestate_groups::PinnedTypeLayout<
                    #group, #ty, #size, { ::core::mem::align_of::<#ty>() }
                >
            },
        };

        tokens.extend(quote!(type #layout = #layout_ty;));
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
