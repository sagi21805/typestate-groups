use proc_macro::TokenStream;
use quote::quote;
use syn::{Ident, ItemImpl, ItemStruct, ItemTrait, parse_macro_input};

use crate::{
    group::Group,
    group_impl::{GroupImpl, GroupImplArgs},
    group_trait::{GroupTrait, GroupTraitArgs},
    state::State,
    state_types::StateTypes,
    typestate::{TypeState, TypeStateArgs},
};

mod group;
mod group_impl;
mod group_trait;
mod naming;
mod state;
mod state_types;
mod syn_ext;
mod typestate;

/// Declares a trait whose associated types group states. A trait without
/// associated types tags its states into groups that set no types.
///
/// ```
/// use typestate_groups::{group, state, state_types};
///
/// #[state_types]
/// trait Meta {
///     type Value;
///     type Label;
/// }
///
/// #[state]
/// struct Small;
/// #[state]
/// struct Big;
///
/// #[group(Numbers)]
/// impl Meta for (Small,) {
///     type Value = u32;
///     type Label = &'static str;
/// }
///
/// #[group(Wide)]
/// impl Meta for (Big,) {
///     type Value = u64;
///     type Label = String;
/// }
/// # fn main() {}
/// ```
#[proc_macro_attribute]
pub fn state_types(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let item_trait = parse_macro_input!(item as ItemTrait);

    let state_types = StateTypes::new(&item_trait);

    quote!(#state_types).into()
}

/// Declares a group and implements a `#[state_types]` trait for its
/// states. `#[size(N)]` on an associated type allows transmuting between
/// them.
///
/// ```
/// use typestate_groups::{group, state, state_types};
///
/// #[state_types]
/// trait Meta {
///     type Value;
/// }
///
/// #[state]
/// struct Small;
/// #[state]
/// struct Tiny;
///
/// #[group(Words)]
/// impl Meta for (Small, Tiny) {
///     #[size(4)]
///     type Value = u32;
/// }
/// # fn main() {}
/// ```
#[proc_macro_attribute]
pub fn group(attr: TokenStream, item: TokenStream) -> TokenStream {
    let item_impl = parse_macro_input!(item as ItemImpl);
    let name = parse_macro_input!(attr as Ident);

    match Group::new(&item_impl, &name) {
        Ok(group) => quote!(#group),
        Err(err) => err.into_compile_error(),
    }
    .into()
}

/// Implements a `#[group_trait]` trait for types whose state is in the
/// group. Name the state with `state = S` when the impl has several type
/// parameters.
///
/// For a trait grouped by several traits, name one group per trait, in
/// any order: `#[group_impl(Meta = Numbers, Role = Heads, state = S)]`.
///
/// `state = Type` implements the trait for one concrete state, such as the
/// only state two groups share. The body then sees that state's own impls,
/// so it can call `morph_mut::<To>()` without a `MorphFrom` bound.
///
/// `#[group_impl(Meta = Numbers, Role = Heads, state = Small)] impl
/// Describe for Wrap<Small>` -> `impl DescribeHelper<Numbers, Heads> for
/// Wrap<Small> where Small: Member<Numbers, Heads>, Small: Meta<Marker =
/// Numbers>, Small: Role<Marker = Heads>, Wrap<Small>: WithState<State =
/// Small>`
///
/// See [`macro@group_trait`] for an example.
#[proc_macro_attribute]
pub fn group_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let item_impl = parse_macro_input!(item as ItemImpl);
    let args = parse_macro_input!(attr as GroupImplArgs);

    match GroupImpl::new(&args, &item_impl) {
        Ok(group_impl) => quote!(#group_impl),
        Err(err) => err.into_compile_error(),
    }
    .into()
}

/// Marks a generic struct as a typestate container. Options:
/// `state = S`, `unsafe_transmute = true`, `align = N`.
///
/// Implements `typestate_groups::Restate` for every target state, so the
/// struct converts with `morph::<S2>()` once it implements
/// `typestate_groups::MorphFrom`.
///
/// ```
/// use typestate_groups::{
///     MorphFrom, Morphic, group, state, state_types, typestate,
/// };
///
/// #[state_types]
/// trait Meta {
///     type Value;
/// }
///
/// #[state]
/// struct Small;
/// #[state]
/// struct Big;
///
/// #[group(SmallGroup)]
/// impl Meta for (Small,) {
///     type Value = u8;
/// }
///
/// #[group(BigGroup)]
/// impl Meta for (Big,) {
///     type Value = u64;
/// }
///
/// #[typestate]
/// struct Wrap<S: Meta> {
///     value: S::Value,
/// }
///
/// impl<S: Meta, S2: Meta> MorphFrom<Wrap<S>> for Wrap<S2>
/// where
///     S2::Value: From<S::Value>,
/// {
///     fn morph_from(src: Wrap<S>) -> Self {
///         Wrap {
///             value: src.value.into(),
///         }
///     }
/// }
///
/// fn main() {
///     let big = Wrap::<Small> { value: 7 }.morph::<Big>();
///     assert_eq!(big.value, 7u64);
/// }
/// ```
#[proc_macro_attribute]
pub fn typestate(attr: TokenStream, item: TokenStream) -> TokenStream {
    let item_struct = parse_macro_input!(item as ItemStruct);
    let args = parse_macro_input!(attr as TypeStateArgs);

    match TypeState::new(args, item_struct) {
        Ok(typestate) => quote!(#typestate),
        Err(err) => err.into_compile_error(),
    }
    .into()
}

/// Implements `typestate_groups::State` for a struct.
///
/// ```
/// use typestate_groups::state;
///
/// #[state]
/// struct Small;
/// # fn main() {}
/// ```
#[proc_macro_attribute]
pub fn state(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let item_struct = parse_macro_input!(item as ItemStruct);
    let state = State::new(&item_struct);

    quote!(#state).into()
}

/// Declares a trait each group implements with `#[group_impl]`.
///
/// The trait can declare methods, associated types and constants, and
/// each group's impl sets its own. `by = (Meta, Role)` groups it by
/// several traits, so each impl covers the states in one group of each.
///
/// ```
/// use typestate_groups::{
///     group, group_impl, group_trait, state, state_types, typestate,
/// };
///
/// #[state_types]
/// trait Meta {
///     type Value;
/// }
///
/// #[state]
/// struct Small;
/// #[state]
/// struct Big;
///
/// #[group(Numbers)]
/// impl Meta for (Small,) {
///     type Value = u32;
/// }
///
/// #[group(Words)]
/// impl Meta for (Big,) {
///     type Value = String;
/// }
///
/// #[typestate]
/// struct Wrap<S: Meta> {
///     value: S::Value,
/// }
///
/// #[group_trait(by = Meta)]
/// trait Describe {
///     fn describe(&self) -> String;
/// }
///
/// #[group_impl(Numbers)]
/// impl<S: Meta> Describe for Wrap<S> {
///     fn describe(&self) -> String {
///         format!("number {}", self.value)
///     }
/// }
///
/// #[group_impl(Words)]
/// impl<S: Meta> Describe for Wrap<S> {
///     fn describe(&self) -> String {
///         format!("word {}", self.value)
///     }
/// }
///
/// fn main() {
///     assert_eq!(Wrap::<Small> { value: 1 }.describe(), "number 1");
///     let big = Wrap::<Big> { value: "hi".into() };
///     assert_eq!(big.describe(), "word hi");
/// }
/// ```
#[proc_macro_attribute]
pub fn group_trait(attr: TokenStream, item: TokenStream) -> TokenStream {
    let item_trait = parse_macro_input!(item as ItemTrait);
    let args = parse_macro_input!(attr as GroupTraitArgs);

    match GroupTrait::new(&args, &item_trait) {
        Ok(group_trait) => quote!(#group_trait),
        Err(err) => err.into_compile_error(),
    }
    .into()
}
