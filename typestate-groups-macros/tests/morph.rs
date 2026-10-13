//! `morph` and `try_morph` rebuild a struct for another state through the
//! user's `MorphFrom` and `TryMorphFrom` impls.

use core::marker::PhantomData;
use typestate_groups::{MorphFrom, Morphic, TryMorphFrom};
use typestate_groups_macros::{group, state, state_types, typestate};

#[state_types]
trait Meta {
    type Value;
}

#[state]
#[derive(Debug, PartialEq)]
struct Small;
#[state]
#[derive(Debug, PartialEq)]
struct Big;

#[group(SmallGroup)]
impl Meta for (Small,) {
    type Value = u8;
}

#[group(BigGroup)]
impl Meta for (Big,) {
    type Value = u64;
}

// --- one generic impl covers every state pair ---

#[typestate]
#[derive(Debug, PartialEq)]
struct Inner<S: Meta> {
    value: S::Value,
}

impl<S: Meta, S2: Meta> MorphFrom<Inner<S>> for Inner<S2>
where
    S2::Value: From<S::Value>,
{
    fn morph_from(src: Inner<S>) -> Self {
        Inner {
            value: src.value.into(),
        }
    }
}

#[typestate]
struct Tag<S: Meta>(PhantomData<S>);

impl<S: Meta, S2: Meta> MorphFrom<Tag<S>> for Tag<S2> {
    fn morph_from(_: Tag<S>) -> Self {
        Tag(PhantomData)
    }
}

#[test]
fn generic_impl_morphs_between_and_within_states() {
    let big = Inner::<Small> { value: 7 }.morph::<Big>();
    assert_eq!(big.value, 7u64);

    let small = Inner::<Small> { value: 7 }.morph::<Small>();
    assert_eq!(small.value, 7u8);

    let Tag(PhantomData) = Tag::<Small>(PhantomData).morph::<Big>();
}

// --- an outer struct reuses the inner struct's conversion ---

#[typestate]
struct Outer<S: Meta> {
    inner: Inner<S>,
    maybe: Option<Inner<S>>,
    many: Vec<Inner<S>>,
    tag: u8,
}

impl<S: Meta, S2: Meta> MorphFrom<Outer<S>> for Outer<S2>
where
    Inner<S2>: MorphFrom<Inner<S>>,
{
    fn morph_from(src: Outer<S>) -> Self {
        Outer {
            inner: src.inner.morph::<S2>(),
            maybe: src.maybe.map(|inner| inner.morph::<S2>()),
            many: src
                .many
                .into_iter()
                .map(|inner| inner.morph::<S2>())
                .collect(),
            tag: src.tag,
        }
    }
}

#[test]
fn outer_struct_reuses_the_inner_conversion() {
    let small = Outer::<Small> {
        inner: Inner { value: 1 },
        maybe: Some(Inner { value: 2 }),
        many: vec![Inner { value: 3 }, Inner { value: 4 }],
        tag: 5,
    };

    let big = small.morph::<Big>();

    assert_eq!(big.inner, Inner { value: 1 });
    assert_eq!(big.maybe, Some(Inner { value: 2 }));
    assert_eq!(big.many, [Inner { value: 3 }, Inner { value: 4 }]);
    assert_eq!(big.tag, 5);
}

// --- `try_morph` through a fallible impl ---

#[typestate]
#[derive(Debug, PartialEq)]
struct Narrow<S: Meta> {
    value: S::Value,
}

impl<S: Meta, S2: Meta> TryMorphFrom<Narrow<S>> for Narrow<S2>
where
    S2::Value: TryFrom<S::Value>,
{
    type Error = <S2::Value as TryFrom<S::Value>>::Error;

    fn try_morph_from(src: Narrow<S>) -> Result<Self, Self::Error> {
        Ok(Narrow {
            value: src.value.try_into()?,
        })
    }
}

#[test]
fn try_morph_fails_only_when_the_value_does_not_fit() {
    let small = Narrow::<Big> { value: 200 }.try_morph::<Small>();
    assert_eq!(small, Ok(Narrow { value: 200u8 }));

    let small = Narrow::<Big> { value: 300 }.try_morph::<Small>();
    assert!(small.is_err());
}

// --- a concrete impl for one pair, with other generics carried over ---

#[typestate(state = S)]
struct Ordered<S: Meta, T> {
    head: u16,
    value: S::Value,
    tail: T,
}

impl<T> MorphFrom<Ordered<Small, T>> for Ordered<Big, T> {
    fn morph_from(src: Ordered<Small, T>) -> Self {
        Ordered {
            head: src.head,
            value: u64::from(src.value) * 2,
            tail: src.tail,
        }
    }
}

#[test]
fn concrete_impl_keeps_other_generics() {
    let small = Ordered::<Small, &str> {
        head: 1,
        value: 2,
        tail: "t",
    };
    let big = small.morph::<Big>();
    assert_eq!((big.head, big.value, big.tail), (1, 4, "t"));
}

// --- `morph_mut` converts where the value lies ---

mod in_place {
    use typestate_groups::MorphFrom;
    use typestate_groups_macros::{group, state, state_types, typestate};

    #[state_types]
    pub trait Count {
        type Total;
    }

    #[state]
    pub struct Open;
    #[state]
    pub struct Closed;

    #[group(OpenGroup)]
    impl Count for (Open,) {
        #[size(4)]
        type Total = u32;
    }

    #[group(ClosedGroup)]
    impl Count for (Closed,) {
        #[size(4)]
        type Total = i32;
    }

    #[typestate(unsafe_transmute = true)]
    pub struct Tally<S: Count> {
        pub total: S::Total,
        pub id: u16,
    }

    impl MorphFrom<Tally<Open>> for Tally<Closed> {
        fn morph_from(src: Tally<Open>) -> Self {
            Tally {
                total: -(src.total as i32),
                id: src.id + 1,
            }
        }
    }
}

#[test]
fn morph_mut_converts_in_place_for_the_owner() {
    use in_place::{Closed, Open, Tally};
    use typestate_groups::Isomorphic;

    fn close(tally: &mut Tally<Open>) -> &mut Tally<Closed> {
        tally.morph_mut::<Closed>()
    }

    let mut tally = Tally::<Open> { total: 3, id: 1 };
    let closed = close(&mut tally);
    assert_eq!((closed.total, closed.id), (-3, 2));

    closed.total = -4;
    let seen = tally.cast_state_ref::<Closed>();
    assert_eq!((seen.total, seen.id), (-4, 2));
}
