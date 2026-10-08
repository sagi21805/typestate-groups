use proc_macro::TokenStream;
use syn::{ItemImpl, ItemStruct, ItemTrait, parse_macro_input};

use crate::{
    group::{Group, GroupArgs},
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

/// Declares a trait whose associated types group states. The trait can
/// take generic parameters. Its group marker trait never does, so a group
/// whose types depend on one lists it, as in `#[group(Owning<T>)]`.
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

    StateTypes::new(&item_trait)
        .and_then(|state_types| state_types.create_group_marker())
        .unwrap_or_else(|err| err.into_compile_error())
        .into()
}

/// Declares a group and implements a `#[state_types]` trait for its
/// states. `#[size(N)]` on an associated type allows transmuting between
/// them. A group whose types depend on an impl parameter lists it, as in
/// `#[group(Buffers<T>)]`.
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
    let args = parse_macro_input!(attr as GroupArgs);

    Group::new(&item_impl, &args)
        .and_then(|group| group.generate_group_impl())
        .unwrap_or_else(|err| err.into_compile_error())
        .into()
}

/// Implements a `#[group_trait]` trait for types whose state is in the
/// group. Name the state with `state = S` when the impl has several type
/// parameters, and a generic group with its arguments, as in
/// `#[group_impl(Buffers<T>, state = S)]`.
///
/// See [`macro@group_trait`] for an example.
#[proc_macro_attribute]
pub fn group_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let item_impl = parse_macro_input!(item as ItemImpl);
    let args = parse_macro_input!(attr as GroupImplArgs);

    GroupImpl::new(&args, &item_impl)
        .map(|group_impl| group_impl.create_group_impl())
        .unwrap_or_else(|err| err.into_compile_error())
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

    TypeState::new(args, item_struct)
        .map(|typestate| typestate.generate_typestate_impls())
        .unwrap_or_else(|err| err.into_compile_error())
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

    State::new(&item_struct).generate_state_impl().into()
}

/// Declares a trait each group implements with `#[group_impl]`.
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

    GroupTrait::new(&args, &item_trait)
        .generate_group_trait()
        .unwrap_or_else(|err| err.into_compile_error())
        .into()
}
