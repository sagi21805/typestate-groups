use extend::ext;
use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use syn::{
    FnArg, GenericParam, Generics, Ident, ItemTrait, Pat, PatIdent,
    PatType, Path, Signature, Token, TraitItem, TraitItemConst,
    TraitItemFn, TraitItemType, TypeParam, WherePredicate, parenthesized,
    parse::{Parse, ParseStream},
    parse_quote,
    punctuated::Punctuated,
    token,
};

use crate::syn_ext::{
    GenericsExt as _, PathArgumentsExt as _, PathExt as _,
};

pub struct GroupTrait<'ast> {
    item_trait: &'ast ItemTrait,
    /// `Meta` and `Role` in `#[group_trait(by = (Meta, Role))]`, ordered
    /// by name.
    keys: &'ast [Path],
    /// `__a_helper_mod::AHelper<'a, <<I as WithState>::State as
    /// Meta>::Marker, <<I as WithState>::State as Role>::Marker, T>`
    helper_bound: Path,
    items: Vec<GroupTraitItem<'ast>>,
}

impl<'ast> GroupTrait<'ast> {
    /// Builds the helper bound and validates the trait's items.
    pub fn new(
        args: &'ast GroupTraitArgs,
        item_trait: &'ast ItemTrait,
    ) -> syn::Result<GroupTrait<'ast>> {
        let keys = &args.keys;
        let implementor = crate::naming::implementor_ident();
        let helper_ident =
            crate::naming::helper_trait_ident(&item_trait.ident);
        let helper_mod_ident =
            crate::naming::helper_mod_ident(&item_trait.ident);

        let mut helper_args = item_trait.generics.to_arguments();
        helper_args.insert_after_lifetimes(keys.iter().map(|key| {
            parse_quote! {
                <<#implementor as ::typestate_groups::WithState>::State as #key>::Marker
            }
        }));

        let items = item_trait
            .items
            .iter()
            .map(GroupTraitItem::try_from)
            .collect::<syn::Result<_>>()?;

        Ok(GroupTrait {
            item_trait,
            keys,
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
    /// #[group_trait(by = (Role, Meta))]
    /// trait Describe<T>: Bound { type Out; fn describe(&self) -> T; }
    /// // ->
    /// trait Describe<T>: Bound { type Out; fn describe(&self) -> T; }
    /// mod __describe_helper_mod {
    ///     trait DescribeHelper<M: MetaGroupMarker, R: RoleGroupMarker, T>: Bound {
    ///         type Out;
    ///         fn describe(&self) -> T;
    ///     }
    ///     trait Member<M: MetaGroupMarker, R: RoleGroupMarker>:
    ///         MetaGroupMember<M> + RoleGroupMember<R> {}
    /// }
    /// impl<T, I> Describe<T> for I
    /// where
    ///     I: WithState,
    ///     <I as WithState>::State: Meta,
    ///     <I as WithState>::State: Role,
    ///     I: DescribeHelper<
    ///         <<I as WithState>::State as Meta>::Marker,
    ///         <<I as WithState>::State as Role>::Marker,
    ///         T,
    ///     >,
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
            keys: self.keys,
        };
        let blanket_impl = BlanketImpl {
            item_trait: self.item_trait,
            keys: self.keys,
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

#[ext(name = GroupKey)]
impl Path {
    /// The helper trait's parameter for a group of this trait.
    ///
    /// `a::Meta` -> `__TypestateGroupsMetaGroup: a::MetaGroupMarker`
    fn group_param(&self) -> GenericParam {
        let group = self.group_param_ident();
        let marker =
            self.with_last_ident(crate::naming::group_marker_ident);
        parse_quote!(#group: #marker)
    }

    /// `a::Meta` -> `__TypestateGroupsMetaGroup`
    fn group_param_ident(&self) -> Ident {
        crate::naming::key_group_param_ident(self.last_ident())
    }

    /// `a::Meta` -> `a::MetaGroupMember<__TypestateGroupsMetaGroup>`
    fn group_member_trait(&self) -> Path {
        let group = self.group_param_ident();
        let member =
            self.with_last_ident(crate::naming::group_member_ident);
        parse_quote!(#member<#group>)
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

/// The helper trait, with one group per key as its first type
/// parameters, and `Member`, which every state in all those groups
/// implements.
struct HelperMod<'a> {
    item_trait: &'a ItemTrait,
    keys: &'a [Path],
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

        let group_params: Vec<GenericParam> =
            self.keys.iter().map(Path::group_param).collect();
        let groups = self.keys.iter().map(Path::group_param_ident);
        let member_traits: Vec<Path> =
            self.keys.iter().map(Path::group_member_trait).collect();

        let mut helper_generics = generics.clone();
        helper_generics.insert_after_lifetimes(group_params.clone());

        tokens.extend(quote! {
            #[doc(hidden)]
            #vis mod #helper_mod_ident {
                use super::*;

                #(#attrs)*
                pub trait #helper_ident #helper_generics #colon_token #supertraits #where_clause {
                    #(#items)*
                }

                pub trait #member_ident<#(#group_params),*>: #(#member_traits)+* {}

                impl<#(#group_params,)* T: #(#member_traits)+*> #member_ident<#(#groups),*> for T {}
            }
        });
    }
}

/// The trait implemented for every state type whose group has a helper
/// impl, forwarding each item to that impl.
struct BlanketImpl<'a> {
    item_trait: &'a ItemTrait,
    keys: &'a [Path],
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
    /// Meta, <I as WithState>::State: Role, I: DescribeHelper<..>`
    fn generics(&self) -> Generics {
        let BlanketImpl {
            keys, helper_bound, ..
        } = self;
        let implementor = crate::naming::implementor_ident();

        let mut generics = self.item_trait.generics.clone();
        generics.params.push(GenericParam::Type(TypeParam::from(
            implementor.clone(),
        )));
        let state_bounds = keys.iter().map(|key| -> WherePredicate {
            parse_quote! {
                <#implementor as ::typestate_groups::WithState>::State: #key
            }
        });
        let predicates = &mut generics.make_where_clause().predicates;
        predicates.push(
            parse_quote!(#implementor: ::typestate_groups::WithState),
        );
        predicates.extend(state_bounds);
        predicates.push(parse_quote!(#implementor: #helper_bound));
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
    /// The traits after `by`, ordered by name.
    keys: Vec<Path>,
}

impl Parse for GroupTraitArgs {
    /// Parses `by = Meta` or `by = (Meta, Role)`, and orders the traits
    /// by name, as `#[group_impl]` orders its `Key = Group` pairs.
    fn parse(input: ParseStream) -> syn::Result<Self> {
        input.parse::<kw::by>()?;
        input.parse::<Token![=]>()?;

        let mut keys: Vec<Path> = if input.peek(token::Paren) {
            let content;
            parenthesized!(content in input);
            Punctuated::<Path, Token![,]>::parse_terminated(&content)?
                .into_iter()
                .collect()
        } else {
            vec![input.parse()?]
        };
        keys.sort_by_cached_key(|key| key.last_ident().to_string());

        if keys.is_empty() {
            return Err(syn::Error::new(
                Span::call_site(),
                "name a trait to group by, such as `by = Meta` or `by = \
                 (Meta, Role)`",
            ));
        }
        if let Some([_, duplicate]) = keys
            .windows(2)
            .find(|pair| pair[0].last_ident() == pair[1].last_ident())
        {
            return Err(syn::Error::new_spanned(
                duplicate,
                format!(
                    "group by `{}` once, and give each trait in `by` a \
                     different name",
                    duplicate.last_ident()
                ),
            ));
        }

        Ok(GroupTraitArgs { keys })
    }
}

mod kw {
    syn::custom_keyword!(by);
}
