use extend::ext;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    FnArg, GenericParam, Ident, ItemTrait, Pat, PatIdent, PatType, Path,
    PathArguments, PathSegment, Signature, Token, TraitItem,
    TraitItemConst, TraitItemFn, TypeParam, TypePath, WherePredicate,
    parse::{Parse, ParseStream},
    parse_quote,
};

use crate::syn_ext::{
    GenericsExt as _, PathArgumentsExt as _, PathExt as _,
};

pub struct GroupTrait<'ast> {
    args: &'ast GroupTraitArgs,
    item_trait: &'ast ItemTrait,
    /// `{Trait}GroupMarker`
    marker_trait: Path,
    /// `{Trait}GroupMember`
    member_trait: Path,
    /// The type the blanket impl implements the trait for.
    implementor: Ident,
    helper_ident: Ident,
    helper_mod_ident: Ident,
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

        let trait_args = item_trait.generics.to_arguments();
        let mut helper_args: PathArguments =
            PathArguments::AngleBracketed(parse_quote!(<#trait_args>));
        helper_args.insert_after_lifetimes(parse_quote! {
            <<#implementor as ::typestate_groups::WithState>::State as #state_types>::Marker
        });
        let helper_bound =
            parse_quote!(#helper_mod_ident::#helper_ident #helper_args);

        GroupTrait {
            args,
            item_trait,
            marker_trait: state_types
                .path
                .with_last_ident(crate::naming::group_marker_ident),
            member_trait: state_types
                .path
                .with_last_ident(crate::naming::group_member_ident),
            implementor,
            helper_ident,
            helper_mod_ident,
            helper_bound,
        }
    }

    /// The trait, its helper module, and a blanket impl forwarding to the
    /// helper impl of the state's group.
    ///
    /// ```ignore
    /// #[group_trait(by = Meta)]
    /// trait Describe<T>: Bound { fn describe(&self) -> T; }
    /// // ->
    /// trait Describe<T>: Bound { fn describe(&self) -> T; }
    /// mod __describe_helper_mod {
    ///     trait DescribeHelper<G: MetaGroupMarker, T>: Bound {
    ///         fn describe(&self) -> T;
    ///     }
    ///     trait Member<G: MetaGroupMarker>: MetaGroupMember<G> {}
    /// }
    /// impl<T, I> Describe<T> for I
    /// where
    ///     I: WithState,
    ///     <I as WithState>::State: Meta,
    ///     I: DescribeHelper<<<I as WithState>::State as Meta>::Marker, T>,
    /// { fn describe(&self) -> T { <I as DescribeHelper<..>>::describe(self) } }
    /// ```
    pub fn generate_group_trait(&self) -> syn::Result<TokenStream> {
        let GroupTrait {
            marker_trait,
            member_trait,
            implementor,
            helper_ident,
            helper_mod_ident,
            helper_bound,
            ..
        } = self;
        let member_ident = crate::naming::helper_member_ident();
        let group = crate::naming::group_param_ident();

        let ItemTrait {
            attrs,
            vis,
            unsafety,
            ident: trait_ident,
            generics,
            colon_token,
            supertraits,
            items,
            ..
        } = self.item_trait;
        let where_clause = &generics.where_clause;

        let declarations = items
            .iter()
            .map(|item| match item {
                TraitItem::Fn(TraitItemFn { attrs, sig, .. }) => {
                    Ok(quote!(#(#attrs)* #sig;))
                }
                TraitItem::Type(_)
                | TraitItem::Const(TraitItemConst {
                    default: None, ..
                }) => Err(syn::Error::new_spanned(
                    item,
                    "remove this item: a `#[group_trait]` trait can hold \
                     only methods and constants with a value",
                )),
                other => Ok(quote!(#other)),
            })
            .collect::<syn::Result<Vec<_>>>()?;

        let delegations = items
            .iter()
            .filter_map(|item| match item {
                TraitItem::Fn(method) => Some(self.delegate(method)),
                _ => None,
            })
            .collect::<syn::Result<Vec<_>>>()?;

        let mut helper_generics = generics.clone();
        helper_generics.params.insert(
            generics.lifetimes().count(),
            parse_quote!(#group: #marker_trait),
        );

        let state_types = &self.args.ty;
        let mut impl_generics = generics.clone();
        impl_generics
            .params
            .push(GenericParam::Type(TypeParam::from(
                implementor.clone(),
            )));
        let implementor_bounds: [WherePredicate; 3] = [
            parse_quote!(#implementor: ::typestate_groups::WithState),
            parse_quote! {
                <#implementor as ::typestate_groups::WithState>::State: #state_types
            },
            parse_quote!(#implementor: #helper_bound),
        ];
        impl_generics
            .make_where_clause()
            .predicates
            .extend(implementor_bounds);
        let (impl_generics, _, impl_where_clause) =
            impl_generics.split_for_impl();
        let (_, ty_generics, _) = generics.split_for_impl();

        Ok(quote! {
            #(#attrs)*
            #unsafety #vis trait #trait_ident #generics #colon_token #supertraits #where_clause {
                #(#declarations)*
            }

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

            impl #impl_generics #trait_ident #ty_generics for #implementor #impl_where_clause {
                #(#delegations)*
            }
        })
    }

    /// `method` with a body that calls the helper trait's version.
    fn delegate(&self, method: &TraitItemFn) -> syn::Result<TokenStream> {
        let sig = &method.sig;
        let args = sig.forwarded_args()?;
        let GroupTrait {
            implementor,
            helper_bound,
            ..
        } = self;
        let method_ident = &sig.ident;

        Ok(quote! {
            #sig {
                <#implementor as #helper_bound>::#method_ident(#(#args),*)
            }
        })
    }
}

/// `#[group_trait]`'s arguments: `by = <Trait>`.
pub struct GroupTraitArgs {
    ty: TypePath,
}

impl Parse for GroupTraitArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        input.parse::<kw::by>()?;
        input.parse::<Token![=]>()?;
        let ty: TypePath = input.parse()?;

        if let Some(PathSegment {
            arguments: arguments @ PathArguments::AngleBracketed(_),
            ..
        }) = ty.path.segments.last()
        {
            return Err(syn::Error::new_spanned(
                arguments,
                "remove the arguments: `#[group_trait]` groups by a \
                 `#[state_types]` trait without generic parameters",
            ));
        }

        Ok(GroupTraitArgs { ty })
    }
}

mod kw {
    syn::custom_keyword!(by);
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
