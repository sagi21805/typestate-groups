//! Declarative macros shared by the crate.

/// Declares one layout property: its marker trait, pinned and unpinned
/// markers, and the `Same*` trait with its impl between equal pinned
/// markers.
macro_rules! layout_property {
    (
        $property:literal: $marker:ident, $pinned:ident<$value:ident>, $unpinned:ident,
        $(#[$same_attr:meta])*
        $same:ident
    ) => {
        #[doc = concat!("The ", $property, " of a type.")]
        #[doc(hidden)]
        pub trait $marker {}

        #[doc = concat!("The ", $property, " of type `T` in group `G`.")]
        #[doc(hidden)]
        pub struct $pinned<G, T, const $value: usize>(PhantomData<(G, T)>);

        impl<G, T, const $value: usize> $marker for $pinned<G, T, $value> {}

        #[doc = concat!("The ", $property, " of type `T` in group `G`, without `#[size(N)]`.")]
        #[doc(hidden)]
        pub struct $unpinned<G, T>(PhantomData<(G, T)>);

        impl<G, T> $marker for $unpinned<G, T> {}

        #[doc = concat!("The type `Self` describes has the same ", $property, " as the type `Other` describes.")]
        $(#[$same_attr])*
        ///
        /// # Safety
        ///
        #[doc = concat!("The described types must have the same ", $property, ".")]
        #[doc(hidden)]
        pub unsafe trait $same<Other: $marker>: $marker {}

        // SAFETY: both carry the same value.
        #[diagnostic::do_not_recommend]
        unsafe impl<G1, T1, G2, T2, const $value: usize>
            $same<$pinned<G2, T2, $value>> for $pinned<G1, T1, $value>
        {
        }
    };
}

/// Implements `Indirect` for each pointer, and `Repointed` between the
/// pointer to `T` and the pointer to `U`.
macro_rules! indirect {
    ($(
        impl<$($lt:lifetime,)? $t:ident, $u:ident> $src:ty => $dst:ty: $aliasing:ident;
    )*) => {$(
        unsafe impl<$($lt,)? $t> $crate::Indirect for $src {
            type Pointee = $t;
            type Aliasing = $crate::$aliasing;
        }

        unsafe impl<$($lt,)? $t, $u> $crate::Repointed<$src> for $dst {}
    )*};
}

/// Declares each receiver marker and implements `CastReceiver` for it.
macro_rules! cast_receiver {
    ($(
        $(#[$attr:meta])*
        $receiver:ident => $borrowed:ident;
    )*) => {$(
        $(#[$attr])*
        pub enum $receiver {}

        impl $crate::CastReceiver for $receiver {
            type Borrowed = $borrowed;
        }
    )*};
}
