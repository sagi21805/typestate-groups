use extend::ext;
use proc_macro2::TokenStream;
use quote::quote;
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
    args: &'ast GroupTraitArgs,
    item_trait: &'ast ItemTrait,
    /// `__a_helper_mod::AHelper<'a, <<I as WithState>::State as
    /// Trait>::Marker, T>`
    helper_bound: Path,
}

impl<'ast> GroupTrait<'ast> {
    pub fn new(
        args: &'ast GroupTraitArgs,
        item_trait: &'ast ItemTrait,
    ) -> GroupTrait<'ast> {
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

        GroupTrait {
            args,
            item_trait,
            helper_bound: parse_quote!(
                #helper_mod_ident::#helper_ident #helper_args
            ),
        }
    }

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
    pub fn generate_group_trait(&self) -> syn::Result<TokenStream> {
        let public_trait = self.public_trait()?;
        let helper_mod = self.helper_mod();
        let blanket_impl = self.blanket_impl()?;

        Ok(quote! {
            #public_trait
            #helper_mod
            #blanket_impl
        })
    }

    /// The trait with its method bodies removed.
    fn public_trait(&self) -> syn::Result<TokenStream> {
        let ItemTrait {
            attrs,
            vis,
            unsafety,
            ident,
            generics,
            colon_token,
            supertraits,
            items,
            ..
        } = self.item_trait;
        let where_clause = &generics.where_clause;

        let declarations = items
            .iter()
            .map(TraitItem::declaration)
            .collect::<syn::Result<Vec<_>>>()?;

        Ok(quote! {
            #(#attrs)*
            #unsafety #vis trait #ident #generics #colon_token #supertraits #where_clause {
                #(#declarations)*
            }
        })
    }

    /// The helper trait, with the group as its first type parameter, and
    /// `Member`, which every state in the group implements.
    fn helper_mod(&self) -> TokenStream {
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

        let state_types = &self.args.ty.path;
        let marker_trait =
            state_types.with_last_ident(crate::naming::group_marker_ident);
        let member_trait =
            state_types.with_last_ident(crate::naming::group_member_ident);

        let mut helper_generics = generics.clone();
        helper_generics.params.insert(
            generics.lifetimes().count(),
            parse_quote!(#group: #marker_trait),
        );

        quote! {
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
        }
    }

    /// The trait implemented for every state type whose group has a helper
    /// impl, forwarding each item to that impl.
    fn blanket_impl(&self) -> syn::Result<TokenStream> {
        let ItemTrait {
            ident, generics, ..
        } = self.item_trait;
        let implementor = crate::naming::implementor_ident();
        let delegations = self.delegations()?;

        let blanket_generics = self.blanket_impl_generics();
        let (impl_generics, _, where_clause) =
            blanket_generics.split_for_impl();
        let (_, ty_generics, _) = generics.split_for_impl();

        Ok(quote! {
            impl #impl_generics #ident #ty_generics for #implementor #where_clause {
                #(#delegations)*
            }
        })
    }

    /// `<T>` -> `<T, I> where I: WithState, <I as WithState>::State:
    /// Meta, I: DescribeHelper<..>`
    fn blanket_impl_generics(&self) -> Generics {
        let state_types = &self.args.ty;
        let helper_bound = &self.helper_bound;
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

    /// Every method, type and constant of the trait, forwarded to the
    /// helper trait.
    fn delegations(&self) -> syn::Result<Vec<TokenStream>> {
        self.item_trait
            .items
            .iter()
            .filter_map(|item| match item {
                TraitItem::Fn(method) => Some(self.delegate_fn(method)),
                TraitItem::Type(ty) => Some(Ok(self.delegate_type(ty))),
                TraitItem::Const(constant) => {
                    Some(Ok(self.delegate_const(constant)))
                }
                _ => None,
            })
            .collect()
    }

    /// `method` with a body that calls the helper trait's version.
    fn delegate_fn(
        &self,
        method: &TraitItemFn,
    ) -> syn::Result<TokenStream> {
        let sig = &method.sig;
        let args = sig.forwarded_args()?;
        let helper_fn = self.helper_item(&sig.ident);

        Ok(quote! {
            #sig {
                #helper_fn(#(#args),*)
            }
        })
    }

    /// `type Ref<'a>: Bound where Self: 'a;` -> `type Ref<'a> = <I as
    /// AHelper<..>>::Ref<'a> where Self: 'a;`
    fn delegate_type(&self, ty: &TraitItemType) -> TokenStream {
        let TraitItemType {
            ident, generics, ..
        } = ty;
        let helper_type = self.helper_item(ident);
        let args = generics.to_arguments();
        let where_clause = &generics.where_clause;

        quote!(type #ident #generics = #helper_type #args #where_clause;)
    }

    /// `const N: usize;` -> `const N: usize = <I as AHelper<..>>::N;`
    fn delegate_const(&self, constant: &TraitItemConst) -> TokenStream {
        let TraitItemConst { ident, ty, .. } = constant;
        let helper_const = self.helper_item(ident);

        quote!(const #ident: #ty = #helper_const;)
    }

    /// `item` -> `<I as AHelper<..>>::item`
    fn helper_item(&self, item: &Ident) -> TokenStream {
        let implementor = crate::naming::implementor_ident();
        let helper_bound = &self.helper_bound;

        quote!(<#implementor as #helper_bound>::#item)
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

#[ext]
impl TraitItem {
    /// This item as the public trait declares it, with a method's body
    /// removed. Types and constants with a default are errors.
    fn declaration(&self) -> syn::Result<TokenStream> {
        match self {
            TraitItem::Fn(TraitItemFn { attrs, sig, .. }) => {
                Ok(quote!(#(#attrs)* #sig;))
            }
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
                self,
                format!(
                    "remove the default and set `{ident}` in each \
                     `#[group_impl]`: `#[group_trait]` doesn't support \
                     default types and constants yet"
                ),
            )),
            other => Ok(quote!(#other)),
        }
    }
}

#[ext]
impl Signature {
    /// `self` and each argument's name.
    fn forwarded_args(&self) -> syn::Result<Vec<TokenStream>> {
        if let Some(asyncness) = &self.asyncness {
            return Err(syn::Error::new_spanned(
                asyncness,
                "remove `async`: `#[group_trait]` doesn't support `async \
                 fn` yet",
            ));
        }

        if self.receiver().is_none() {
            return Err(syn::Error::new_spanned(
                self,
                format!(
                    "add a `self` receiver to `{}`: every \
                     `#[group_trait]` method needs one",
                    self.ident
                ),
            ));
        }

        self.inputs
            .iter()
            .enumerate()
            .map(|(index, fn_arg)| match fn_arg {
                FnArg::Receiver(_) => Ok(quote!(self)),
                FnArg::Typed(PatType { pat, .. }) => match &**pat {
                    Pat::Ident(PatIdent { ident, .. }) => {
                        Ok(quote!(#ident))
                    }
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
            })
            .collect()
    }
}
