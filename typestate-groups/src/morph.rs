//! By-value transitions between a container's states.

use crate::{Restate, State, WithState};

/// Builds `Self` from `Src`, the same container in another state.
///
/// Implement it once, generic over both states, or per pair of states.
/// Reuse a field's own conversion with `morph`.
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
/// struct Inner<S: Meta> {
///     value: S::Value,
/// }
///
/// impl<S: Meta, S2: Meta> MorphFrom<Inner<S>> for Inner<S2>
/// where
///     S2::Value: From<S::Value>,
/// {
///     fn morph_from(src: Inner<S>) -> Self {
///         Inner {
///             value: src.value.into(),
///         }
///     }
/// }
///
/// #[typestate]
/// struct Outer<S: Meta> {
///     inner: Inner<S>,
///     maybe: Option<Inner<S>>,
///     tag: u8,
/// }
///
/// impl<S: Meta, S2: Meta> MorphFrom<Outer<S>> for Outer<S2>
/// where
///     Inner<S2>: MorphFrom<Inner<S>>,
/// {
///     fn morph_from(src: Outer<S>) -> Self {
///         Outer {
///             inner: src.inner.morph::<S2>(),
///             maybe: src.maybe.map(|inner| inner.morph::<S2>()),
///             tag: src.tag,
///         }
///     }
/// }
///
/// let small = Outer::<Small> {
///     inner: Inner { value: 1 },
///     maybe: Some(Inner { value: 2 }),
///     tag: 3,
/// };
/// let big = small.morph::<Big>();
/// assert_eq!(big.maybe.map(|inner| inner.value), Some(2u64));
/// ```
#[diagnostic::on_unimplemented(
    message = "implement `typestate_groups::MorphFrom<{Src}>` for \
               `{Self}` to morph `{Src}` into it"
)]
pub trait MorphFrom<Src: WithState>: WithState {
    /// Converts `src` into `Self`.
    fn morph_from(src: Src) -> Self;
}

/// Tries to build `Self` from `Src`, the same container in another state.
///
/// The fallible [`MorphFrom`]. A pair can implement both.
#[diagnostic::on_unimplemented(
    message = "implement `typestate_groups::TryMorphFrom<{Src}>` for \
               `{Self}` to try morphing `{Src}` into it"
)]
pub trait TryMorphFrom<Src: WithState>: WithState + Sized {
    /// Why the conversion failed.
    type Error;

    /// Converts `src` into `Self`, or fails with [`Self::Error`].
    fn try_morph_from(src: Src) -> Result<Self, Self::Error>;
}

/// Converts a container into the same container in another [`State`],
/// through [`MorphFrom`] or [`TryMorphFrom`].
///
/// ```ignore
/// let big = small.morph::<Big>(); // Wrap<Small> -> Wrap<Big>
/// ```
pub trait Morphic: WithState + Sized {
    /// Converts `self` into state `To`.
    fn morph<To: State>(self) -> <Self as Restate<To>>::Target
    where
        Self: Restate<To>,
        <Self as Restate<To>>::Target: MorphFrom<Self>,
    {
        MorphFrom::morph_from(self)
    }

    /// Tries to convert `self` into state `To`.
    fn try_morph<To: State>(
        self,
    ) -> Result<
        <Self as Restate<To>>::Target,
        <<Self as Restate<To>>::Target as TryMorphFrom<Self>>::Error,
    >
    where
        Self: Restate<To>,
        <Self as Restate<To>>::Target: TryMorphFrom<Self>,
    {
        TryMorphFrom::try_morph_from(self)
    }
}

impl<T: WithState> Morphic for T {}
