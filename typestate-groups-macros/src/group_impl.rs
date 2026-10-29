use extend::ext;
use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use syn::{
    AssocType, Generics, Ident, ItemImpl, Path, PathSegment, Token, Type,
    TypePath, WherePredicate,
    parse::{Parse, ParseStream},
    parse_quote,
};

use crate::syn_ext::{
    GenericsExt as _, ItemImplExt as _, OptionExt as _,
    PathArgumentsExt as _, TypeExt as _,
};

pub struct GroupImpl<'ast> {
    inner_impl: &'ast ItemImpl,
    /// Ordered by key, as `#[group_trait]` orders its keys.
    groups: &'ast [ImplGroup],
    state: ImplState<'ast>,
}

impl<'ast> GroupImpl<'ast> {
    /// Resolves the state of a trait impl.
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

        let state = match &args.state {
            Some(state) => ImplState::new(state, &item_impl.generics),
            None => {
                let mut type_params =
                    item_impl.generics.type_params().map(|tp| &tp.ident);
                type_params
                    .next()
                    .filter(|_| type_params.next().is_none())
                    .map(ImplState::Param)
                    .ok_or_else(|| {
                        syn::Error::new_spanned(
                            &item_impl.generics,
                            "add `state = <Ident>` to `#[group_impl]` to \
                             name the state parameter, or `state = \
                             <Type>` to name one concrete state; only an \
                             impl with one generic type parameter can \
                             leave it out",
                        )
                    })?
            }
        };

        Ok(GroupImpl {
            inner_impl: item_impl,
            groups: &args.groups,
            state,
        })
    }
}

impl ToTokens for GroupImpl<'_> {
    /// `impl<S> A<'a, X> for T<S>` -> `impl<S> __a_helper_mod::AHelper<'a,
    /// Group, X> for T<S> where S: __a_helper_mod::Member<Group>`
    ///
    /// Each `Key = Group` pair also bounds `S: Key<Marker = Group>`, so a
    /// group named under the wrong key fails there. A concrete state adds
    /// `T<S>: WithState<State = S>`.
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let mut group_impl = self.inner_impl.clone();
        let bindings = match self.state {
            ImplState::Param(param) => {
                group_impl.generics.take_assoc_type_bindings(param)
            }
            ImplState::Concrete(_) => Vec::new(),
        };

        let (trait_path, _) = group_impl
            .trait_
            .as_mut()
            .expect("`GroupImpl::new` accepts only trait impls");
        let helper_trait = self.helper_trait_path(trait_path);
        *trait_path = helper_trait.clone();
        let key_bounds = self
            .groups
            .iter()
            .filter_map(|group| group.key_bound(&self.state));
        let predicates =
            &mut group_impl.generics.make_where_clause().predicates;
        predicates.push(self.member_bound(&helper_trait));
        predicates.extend(key_bounds);
        predicates.extend(self.concrete_state_bound());
        group_impl.qualify_self_paths(&helper_trait);

        let binding_errors = bindings.iter().map(|binding| {
            self.binding_error(binding).into_compile_error()
        });

        tokens.extend(quote! {
            #group_impl

            #(#binding_errors)*
        });
    }
}

impl GroupImpl<'_> {
    /// `a::A<'a, X>` -> `a::__a_helper_mod::AHelper<'a, Group, X>`
    fn helper_trait_path(&self, trait_path: &Path) -> Path {
        let leading_colon = trait_path.leading_colon;
        let mut segments = trait_path.segments.clone();
        let PathSegment {
            ident,
            mut arguments,
        } = segments
            .pop()
            .expect("a parsed trait path has at least one segment");

        let helper_mod = crate::naming::helper_mod_ident(&ident);
        let helper_trait = crate::naming::helper_trait_ident(&ident);
        for ImplGroup { group, .. } in self.groups.iter().rev() {
            arguments.insert_after_lifetimes(parse_quote!(#group));
        }
        let prefix = segments.iter();

        parse_quote! {
            #leading_colon #(#prefix::)* #helper_mod::#helper_trait #arguments
        }
    }

    /// `a::__a_helper_mod::AHelper<..>` -> `S:
    /// a::__a_helper_mod::Member<Group>`
    ///
    /// The bound names the state parameter, not `<Self as
    /// WithState>::State`, because a `where` clause implying `Self:
    /// WithState` hides the impl that resolves that projection to `S`.
    fn member_bound(&self, helper_trait: &Path) -> WherePredicate {
        let groups = self.groups.iter().map(|group| &group.group);
        let member_ident = crate::naming::helper_member_ident();
        let mut member = helper_trait.clone();
        *member
            .segments
            .last_mut()
            .expect("a parsed trait path has at least one segment") =
            parse_quote!(#member_ident<#(#groups),*>);

        let state = &self.state;
        parse_quote! { #state: #member }
    }

    /// `T<Attached>: WithState<State = Attached>` for a concrete state, so
    /// an impl for a state other than `state` fails. Without it, the
    /// helper impl would never be used and nothing would report it.
    fn concrete_state_bound(&self) -> Option<WherePredicate> {
        let ImplState::Concrete(state) = self.state else {
            return None;
        };
        let self_ty = &self.inner_impl.self_ty;
        Some(parse_quote! {
            #self_ty: ::typestate_groups::WithState<State = #state>
        })
    }

    /// The error for a binding on the state, which the group already sets
    /// or which belongs to a trait the impl isn't grouped by.
    fn binding_error(&self, binding: &AssocType) -> syn::Error {
        let groups = self
            .groups
            .iter()
            .map(|group| format!("`{}`", group.group))
            .collect::<Vec<_>>()
            .join(", ");

        syn::Error::new_spanned(
            binding,
            format!(
                "remove `{}`, because `#[group_impl]` takes the state's \
                 types from {groups}. To pin a type they don't set, \
                 declare it on a trait they group by, or group this \
                 trait by the trait that declares it",
                quote!(#binding),
            ),
        )
    }
}

/// `#[group_impl(Group, state = S)]` or `#[group_impl(Meta = Group,
/// Role = Group, state = S)]`
pub struct GroupImplArgs {
    /// Ordered by key, as `#[group_trait]` orders its keys.
    groups: Vec<ImplGroup>,
    state: Option<Type>,
}

impl Parse for GroupImplArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut groups = Vec::new();
        let mut state: Option<Type> = None;

        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            match ident.to_string().as_str() {
                "state" => state.parse_once(&ident, input)?,
                _ => groups.insert_by_key(ImplGroup::parse_after(
                    ident, input,
                )?)?,
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        if groups.is_empty() {
            return Err(syn::Error::new(
                Span::call_site(),
                "name the group, such as `#[group_impl(Numbers)]`",
            ));
        }
        Ok(GroupImplArgs { groups, state })
    }
}

#[ext]
impl Vec<ImplGroup> {
    /// Inserts `group` in key order, rejecting a group without a key next
    /// to others and a key given twice.
    fn insert_by_key(&mut self, group: ImplGroup) -> syn::Result<()> {
        if !self.is_empty()
            && let Some(unnamed) = self
                .first()
                .into_iter()
                .chain([&group])
                .find(|group| group.key.is_none())
        {
            return Err(syn::Error::new(
                unnamed.group.span(),
                "name the trait each group belongs to, such as `Meta = \
                 Numbers`, when the impl has more than one group",
            ));
        }

        match self.binary_search_by(|probe| probe.key.cmp(&group.key)) {
            Ok(_) => {
                let key = group
                    .key
                    .as_ref()
                    .expect("a group without a key stands alone");
                Err(syn::Error::new(
                    key.span(),
                    format!("name one group for `{key}`"),
                ))
            }
            Err(index) => {
                self.insert(index, group);
                Ok(())
            }
        }
    }
}

/// A group of `#[group_impl]`, written `Group`, or `Meta = Group` to
/// name the trait it groups by.
struct ImplGroup {
    key: Option<Ident>,
    group: Ident,
}

impl ImplGroup {
    /// Parses the rest of `Meta = Group`, or takes `ident` as the group
    /// when no `=` follows it.
    fn parse_after(ident: Ident, input: ParseStream) -> syn::Result<Self> {
        if !input.peek(Token![=]) {
            return Ok(ImplGroup {
                key: None,
                group: ident,
            });
        }
        input.parse::<Token![=]>()?;

        Ok(ImplGroup {
            key: Some(ident),
            group: input.parse()?,
        })
    }

    /// `Meta = Group` -> `S: Meta<Marker = Group>`
    fn key_bound(&self, state: &ImplState) -> Option<WherePredicate> {
        let ImplGroup { key, group } = self;
        let key = key.as_ref()?;

        Some(parse_quote!(#state: #key<Marker = #group>))
    }
}

/// The state a `#[group_impl]` covers.
enum ImplState<'ast> {
    /// A generic type parameter of the impl, one state per group member.
    Param(&'ast Ident),
    /// One concrete state type.
    Concrete(&'ast Type),
}

impl<'ast> ImplState<'ast> {
    /// `state = S` naming one of the impl's type parameters is a `Param`,
    /// and any other type is `Concrete`.
    fn new(state: &'ast Type, generics: &'ast Generics) -> Self {
        let param = match state.peeled() {
            Type::Path(TypePath {
                qself: None, path, ..
            }) => path.get_ident().and_then(|ident| {
                generics
                    .type_params()
                    .map(|tp| &tp.ident)
                    .find(|param| *param == ident)
            }),
            _ => None,
        };
        param.map_or(ImplState::Concrete(state), ImplState::Param)
    }
}

impl ToTokens for ImplState<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            ImplState::Param(param) => param.to_tokens(tokens),
            ImplState::Concrete(state) => state.to_tokens(tokens),
        }
    }
}
