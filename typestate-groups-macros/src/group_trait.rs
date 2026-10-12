use extend::ext;
use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::{
    FnArg, GenericParam, Generics, Ident, ItemTrait, Pat, PatIdent,
    PatType, Path, Signature, Token, TraitItem, TraitItemConst,
    TraitItemFn, TraitItemType, TypeParam, TypePath, WherePredicate,
    parse::{Parse, ParseStream},
    parse_quote,
};

use crate::syn_ext::{
    GenericsExt as _, PathArgumentsExt as _, PathExt as _,
};

pub struct GroupTrait<'ast> {
    item_trait: &'ast ItemTrait,
    /// `Meta` in `#[group_trait(by = Meta)]`.
    state_types: &'ast TypePath,
    /// `__a_helper_mod::AHelper<'a, <<I as WithState>::State as
    /// Trait>::Marker, T>`
    helper_bound: Path,
    items: Vec<GroupTraitItem<'ast>>,
}

impl<'ast> GroupTrait<'ast> {
    /// Builds the helper bound and validates the trait's items.
    pub fn new(
        args: &'ast GroupTraitArgs,
        item_trait: &'ast ItemTrait,
    ) -> syn::Result<GroupTrait<'ast>> {
        let state_types = &args.ty;
        let implementor = crate::naming::implementor_ident();
        let helper_ident =
            crate::naming::helper_trait_ident(&item_trait.ident);
        let helper_mod_ident =
            crate::naming::helper_mod_ident(&item_trait.ident);

        let mut helper_args = item_trait.generics.to_arguments();
        helper_args.insert_after_lifetimes(parse_quote! {
            <<#implementor as ::typestate_groups::WithState>::State as #state_types>::Marker
        });

        let items = item_trait
            .items
            .iter()
            .map(GroupTraitItem::try_from)
            .collect::<syn::Result<_>>()?;

        Ok(GroupTrait {
            item_trait,
            state_types,
            helper_bound: parse_quote!(
                #helper_mod_ident::#helper_ident #helper_args
            ),
            items,
        })
    }
}

impl ToTokens for GroupTrait<'_> {
    /// The trait, its helper module, and a blanket impl forwarding to the
    /// helper impl of the state's group.
    ///
    /// ```ignore
    /// #[group_trait(by = Meta)]
    /// trait Describe<T>: Bound { type Out; fn describe(&self) -> T; }
    /// // ->
    /// trait Describe<T>: Bound { type Out; fn describe(&self) -> T; }
    /// mod __describe_helper_mod {
    ///     trait DescribeHelper<G: MetaGroupMarker, T>: Bound {
    ///         type Out;
    ///         fn describe(&self) -> T;
    ///     }
    ///     trait Member<G: MetaGroupMarker>: MetaGroupMember<G> {}
    /// }
    /// impl<T, I> Describe<T> for I
    /// where
    ///     I: WithState,
    ///     <I as WithState>::State: Meta,
    ///     I: DescribeHelper<<<I as WithState>::State as Meta>::Marker, T>,
    /// {
    ///     type Out = <I as DescribeHelper<..>>::Out;
    ///     fn describe(&self) -> T { <I as DescribeHelper<..>>::describe(self) }
    /// }
    /// ```
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let public_trait = PublicTrait {
            item_trait: self.item_trait,
            items: &self.items,
        };
        let helper_mod = HelperMod {
            item_trait: self.item_trait,
            state_types: &self.state_types.path,
        };
        let blanket_impl = BlanketImpl {
            item_trait: self.item_trait,
            state_types: self.state_types,
            helper_bound: &self.helper_bound,
            items: &self.items,
        };

        tokens.extend(quote! {
            #public_trait
            #helper_mod
            #blanket_impl
        });
    }
}

/// The trait with its method bodies removed.
struct PublicTrait<'a> {
    item_trait: &'a ItemTrait,
    items: &'a [GroupTraitItem<'a>],
}

impl ToTokens for PublicTrait<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let ItemTrait {
            attrs,
            vis,
            unsafety,
            ident,
            generics,
            colon_token,
            supertraits,
            ..
        } = self.item_trait;
        let where_clause = &generics.where_clause;

        let declarations =
            self.items.iter().map(|item| Declaration { item });

        tokens.extend(quote! {
            #(#attrs)*
            #unsafety #vis trait #ident #generics #colon_token #supertraits #where_clause {
                #(#declarations)*
            }
        });
    }
}

/// The helper trait, with the group as its first type parameter, and
/// `Member`, which every state in the group implements.
struct HelperMod<'a> {
    item_trait: &'a ItemTrait,
    state_types: &'a Path,
}

impl ToTokens for HelperMod<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let ItemTrait {
            attrs,
            vis,
            ident,
            generics,
            colon_token,
            supertraits,
            items,
            ..
        } = self.item_trait;
        let where_clause = &generics.where_clause;
        let helper_ident = crate::naming::helper_trait_ident(ident);
        let helper_mod_ident = crate::naming::helper_mod_ident(ident);
        let member_ident = crate::naming::helper_member_ident();
        let group = crate::naming::group_param_ident();

        let state_types = self.state_types;
        let marker_trait =
            state_types.with_last_ident(crate::naming::group_marker_ident);
        let member_trait =
            state_types.with_last_ident(crate::naming::group_member_ident);

        let mut helper_generics = generics.clone();
        helper_generics.params.insert(
            generics.lifetimes().count(),
            parse_quote!(#group: #marker_trait),
        );

        tokens.extend(quote! {
            #[doc(hidden)]
            #vis mod #helper_mod_ident {
                use super::*;

                #(#attrs)*
                pub trait #helper_ident #helper_generics #colon_token #supertraits #where_clause {
                    #(#items)*
                }

                pub trait #member_ident<#group: #marker_trait>: #member_trait<#group> {}

                impl<#group: #marker_trait, T: #member_trait<#group>> #member_ident<#group> for T {}
            }
        });
    }
}

/// The trait implemented for every state type whose group has a helper
/// impl, forwarding each item to that impl.
struct BlanketImpl<'a> {
    item_trait: &'a ItemTrait,
    state_types: &'a TypePath,
    helper_bound: &'a Path,
    items: &'a [GroupTraitItem<'a>],
}

impl ToTokens for BlanketImpl<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let ItemTrait {
            ident, generics, ..
        } = self.item_trait;
        let implementor = crate::naming::implementor_ident();
        let delegations = self.items.iter().map(|item| Delegation {
            item,
            helper_bound: self.helper_bound,
        });

        let blanket_generics = self.generics();
        let (impl_generics, _, where_clause) =
            blanket_generics.split_for_impl();
        let (_, ty_generics, _) = generics.split_for_impl();

        tokens.extend(quote! {
            impl #impl_generics #ident #ty_generics for #implementor #where_clause {
                #(#delegations)*
            }
        });
    }
}

impl BlanketImpl<'_> {
    /// `<T>` -> `<T, I> where I: WithState, <I as WithState>::State:
    /// Meta, I: DescribeHelper<..>`
    fn generics(&self) -> Generics {
        let BlanketImpl {
            state_types,
            helper_bound,
            ..
        } = self;
        let implementor = crate::naming::implementor_ident();

        let mut generics = self.item_trait.generics.clone();
        generics.params.push(GenericParam::Type(TypeParam::from(
            implementor.clone(),
        )));
        let implementor_bounds: [WherePredicate; 3] = [
            parse_quote!(#implementor: ::typestate_groups::WithState),
            parse_quote! {
                <#implementor as ::typestate_groups::WithState>::State: #state_types
            },
            parse_quote!(#implementor: #helper_bound),
        ];
        generics
            .make_where_clause()
            .predicates
            .extend(implementor_bounds);
        generics
    }
}

/// An item forwarded to the helper trait's version of it.
///
/// `const N: usize;` -> `const N: usize = <I as AHelper<..>>::N;`
struct Delegation<'a> {
    item: &'a GroupTraitItem<'a>,
    helper_bound: &'a Path,
}

impl ToTokens for Delegation<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self.item {
            GroupTraitItem::Fn {
                method: TraitItemFn { sig, .. },
                args,
            } => {
                let helper_fn = self.helper_item(&sig.ident);
                tokens.extend(quote! {
                    #sig {
                        #helper_fn(self, #(#args),*)
                    }
                });
            }
            GroupTraitItem::Type(TraitItemType {
                ident,
                generics,
                ..
            }) => {
                let helper_type = self.helper_item(ident);
                let args = generics.to_arguments();
                let where_clause = &generics.where_clause;
                tokens.extend(quote! {
                    type #ident #generics = #helper_type #args #where_clause;
                });
            }
            GroupTraitItem::Const(TraitItemConst {
                ident, ty, ..
            }) => {
                let helper_const = self.helper_item(ident);
                tokens.extend(quote!(const #ident: #ty = #helper_const;));
            }
            GroupTraitItem::Other(_) => {}
        }
    }
}

impl<'a> Delegation<'a> {
    /// `item` -> `<I as AHelper<..>>::item`
    fn helper_item(&self, item: &'a Ident) -> HelperItem<'a> {
        HelperItem {
            helper_bound: self.helper_bound,
            item,
        }
    }
}

/// `<I as AHelper<..>>::item`, a trait item reached through the helper
/// trait.
struct HelperItem<'a> {
    helper_bound: &'a Path,
    item: &'a Ident,
}

impl ToTokens for HelperItem<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let implementor = crate::naming::implementor_ident();
        let HelperItem { helper_bound, item } = self;

        tokens.extend(quote!(<#implementor as #helper_bound>::#item));
    }
}

/// `#[group_trait]`'s arguments: `by = <state>`.
pub struct GroupTraitArgs {
    ty: TypePath,
}

impl Parse for GroupTraitArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        input.parse::<kw::by>()?;
        input.parse::<Token![=]>()?;

        Ok(GroupTraitArgs { ty: input.parse()? })
    }
}

mod kw {
    syn::custom_keyword!(by);
}

/// A trait item `#[group_trait]` accepts.
enum GroupTraitItem<'a> {
    /// A method with a `self` receiver, and the names of its other
    /// arguments.
    Fn {
        method: &'a TraitItemFn,
        args: Vec<&'a Ident>,
    },
    /// An associated type without a default.
    Type(&'a TraitItemType),
    /// An associated constant without a default.
    Const(&'a TraitItemConst),
    /// Any other item, declared but not forwarded.
    Other(&'a TraitItem),
}

impl<'a> TryFrom<&'a TraitItem> for GroupTraitItem<'a> {
    type Error = syn::Error;

    /// Rejects types and constants with a default, and methods whose
    /// arguments can't be forwarded.
    fn try_from(item: &'a TraitItem) -> syn::Result<Self> {
        match item {
            TraitItem::Fn(method) => Ok(GroupTraitItem::Fn {
                method,
                args: method.sig.arg_names()?,
            }),
            TraitItem::Type(TraitItemType {
                ident,
                default: Some(_),
                ..
            })
            | TraitItem::Const(TraitItemConst {
                ident,
                default: Some(_),
                ..
            }) => Err(syn::Error::new_spanned(
                item,
                format!(
                    "remove the default and set `{ident}` in each \
                     `#[group_impl]`: `#[group_trait]` doesn't support \
                     default types and constants yet"
                ),
            )),
            TraitItem::Type(ty) => Ok(GroupTraitItem::Type(ty)),
            TraitItem::Const(constant) => {
                Ok(GroupTraitItem::Const(constant))
            }
            other => Ok(GroupTraitItem::Other(other)),
        }
    }
}

/// An item as the public trait declares it, with a method's body
/// removed.
struct Declaration<'a> {
    item: &'a GroupTraitItem<'a>,
}

impl ToTokens for Declaration<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self.item {
            GroupTraitItem::Fn {
                method: TraitItemFn { attrs, sig, .. },
                ..
            } => tokens.extend(quote!(#(#attrs)* #sig;)),
            GroupTraitItem::Type(ty) => ty.to_tokens(tokens),
            GroupTraitItem::Const(constant) => constant.to_tokens(tokens),
            GroupTraitItem::Other(item) => item.to_tokens(tokens),
        }
    }
}

#[ext]
impl Signature {
    /// The name of each argument after `self`.
    fn arg_names(&self) -> syn::Result<Vec<&Ident>> {
        if let Some(asyncness) = &self.asyncness {
            return Err(syn::Error::new_spanned(
                asyncness,
                "remove `async`: `#[group_trait]` doesn't support `async \
                 fn` yet",
            ));
        }

        let mut inputs = self.inputs.iter().enumerate();
        let Some((_, FnArg::Receiver(_))) = inputs.next() else {
            return Err(syn::Error::new_spanned(
                self,
                format!(
                    "add a `self` receiver to `{}`: every \
                     `#[group_trait]` method needs one",
                    self.ident
                ),
            ));
        };

        inputs
            .map(|(index, fn_arg)| match fn_arg {
                FnArg::Typed(PatType { pat, .. }) => match &**pat {
                    Pat::Ident(PatIdent { ident, .. }) => Ok(ident),
                    other => Err(syn::Error::new_spanned(
                        other,
                        format!(
                            "bind argument {index} of `{}` to a plain \
                             name, such as `value: T`: `#[group_trait]` \
                             forwards arguments by name",
                            self.ident
                        ),
                    )),
                },
                FnArg::Receiver(_) => unreachable!(
                    "syn parses a receiver only as the first argument"
                ),
            })
            .collect()
    }
}
