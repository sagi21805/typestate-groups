use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    AssocType, Ident, ItemImpl, Path, PathSegment, Token,
    parse::{Parse, ParseStream},
    parse_quote,
};

use crate::syn_ext::{
    GenericsExt as _, OptionExt as _, PathArgumentsExt as _,
};

pub struct GroupImpl<'ast> {
    inner_impl: &'ast ItemImpl,
    /// The group, such as `FreeList<T>`.
    group: &'ast Path,
    /// The impl's generic type parameter carrying the state.
    state: &'ast Ident,
}

impl<'ast> GroupImpl<'ast> {
    /// Resolves the state parameter of a trait impl.
    pub fn new(
        args: &'ast GroupImplArgs,
        item_impl: &'ast ItemImpl,
    ) -> syn::Result<GroupImpl<'ast>> {
        if item_impl.trait_.is_none() {
            return Err(syn::Error::new_spanned(
                item_impl,
                "use `#[group_impl]` on a trait impl, such as `impl \
                 Trait for Wrap<S>`",
            ));
        }

        let mut type_params =
            item_impl.generics.type_params().map(|tp| &tp.ident);

        let state = match &args.state {
            Some(state) => type_params
                .find(|ident| *ident == state)
                .ok_or_else(|| {
                    syn::Error::new(
                        state.span(),
                        "set `state` to one of the impl's generic type \
                         parameters",
                    )
                })?,
            None => type_params
                .next()
                .filter(|_| type_params.next().is_none())
                .ok_or_else(|| {
                    syn::Error::new_spanned(
                        &item_impl.generics,
                        "add `state = <Ident>` to `#[group_impl]` to \
                         name the state parameter; only an impl with one \
                         generic type parameter can leave it out",
                    )
                })?,
        };

        Ok(GroupImpl {
            inner_impl: item_impl,
            group: &args.group,
            state,
        })
    }

    /// `impl A<'a, X> for T -> impl __a_helper_mod::AHelper<'a, Group, X>
    /// for T where T::State: __a_helper_mod::Member<Group>`
    ///
    /// The group sets the state's associated types, so every `Assoc =
    /// Type` binding on the state becomes an error. The impl is still
    /// emitted without them, so its body keeps its types while the user
    /// fixes it.
    pub fn create_group_impl(&self) -> TokenStream {
        let group = self.group;
        let mut modified = self.inner_impl.clone();
        let bindings =
            modified.generics.take_assoc_type_bindings(self.state);

        let (trait_path, _) = modified
            .trait_
            .as_mut()
            .expect("`GroupImpl::new` accepts only trait impls");

        let last = trait_path
            .segments
            .last_mut()
            .expect("a parsed trait path has at least one segment");

        let helper_mod_ident =
            crate::naming::helper_mod_ident(&last.ident);
        last.ident = crate::naming::helper_trait_ident(&last.ident);
        last.arguments.insert_after_lifetimes(parse_quote!(#group));

        let mod_index = trait_path.segments.len() - 1;
        trait_path
            .segments
            .insert(mod_index, PathSegment::from(helper_mod_ident));

        let member_ident = crate::naming::helper_member_ident();
        let mut member = trait_path.clone();
        *member
            .segments
            .last_mut()
            .expect("a parsed trait path has at least one segment") =
            parse_quote!(#member_ident<#group>);
        modified.generics.make_where_clause().predicates.push(
            parse_quote! {
                <Self as ::typestate_groups::WithState>::State: #member
            },
        );

        let binding_errors = bindings.iter().map(|binding| {
            self.binding_error(binding).into_compile_error()
        });

        quote! {
            #modified

            #(#binding_errors)*
        }
    }

    /// The error for a binding on the state, which the group already sets
    /// or which belongs to a trait the impl isn't grouped by.
    fn binding_error(&self, binding: &AssocType) -> syn::Error {
        let group = &self
            .group
            .segments
            .last()
            .expect("a parsed path has at least one segment")
            .ident;

        syn::Error::new_spanned(
            binding,
            format!(
                "remove `{}`: `#[group_impl({group})]` takes the state's \
                 types from `{group}`. To pin a type `{group}` doesn't \
                 set, declare it on the trait `{group}` groups by, or \
                 group this trait by the trait that declares it",
                quote!(#binding),
            ),
        )
    }
}

/// `#[group_impl(Group<T>, state = S)]`
pub struct GroupImplArgs {
    group: Path,
    state: Option<Ident>,
}

impl Parse for GroupImplArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let group = input.parse()?;
        let mut state: Option<Ident> = None;

        while !input.is_empty() {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }

            let key: Ident = input.parse()?;
            match key.to_string().as_str() {
                "state" => state.parse_once(&key, input)?,
                _ => {
                    return Err(syn::Error::new(
                        key.span(),
                        "expected `state = <Ident>`",
                    ));
                }
            }
        }

        Ok(GroupImplArgs { group, state })
    }
}
