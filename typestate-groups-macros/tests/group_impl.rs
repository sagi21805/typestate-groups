//! `#[group_trait]` and `#[group_impl]` dispatch a trait to one impl per
//! group.
#![expect(
    non_camel_case_types,
    reason = "a snake_case trait checks the generated helper names"
)]

use typestate_groups::{Isomorphic, State, WithState};
use typestate_groups_macros::{
    group, group_impl, group_trait, state, state_types, typestate,
};

#[state_types]
trait Meta {
    type Value;
    type Extra;
}

#[state]
struct A;
#[state]
struct B;
#[state]
struct C;

#[group(Numbers)]
impl Meta for (A, B) {
    type Value = u32;
    type Extra = u8;
}

#[group(Words)]
impl Meta for (C,) {
    type Value = String;
    type Extra = char;
}

#[typestate]
struct Wrap<S: Meta> {
    value: S::Value,
    extra: S::Extra,
}

#[group_trait(by = Meta)]
trait describe {
    const LIMIT: u32;

    fn sum(&self, x: u32, y: u32) -> String;

    fn tag(&self) -> &'static str {
        "default"
    }

    fn doubled(&self, x: u32) -> String {
        self.sum(x, x)
    }
}

// Only `S: Meta`: `#[group_impl]` pins `S::Value` and `S::Extra` to the
// group's types.
#[group_impl(Numbers)]
impl<S: Meta> describe for Wrap<S> {
    const LIMIT: u32 = 42;

    fn sum(&self, x: u32, y: u32) -> String {
        (self.value + u32::from(self.extra) + x + y).to_string()
    }

    fn tag(&self) -> &'static str {
        "numbers"
    }
}

#[group_impl(Words)]
impl<S: Meta + State> describe for Wrap<S> {
    const LIMIT: u32 = 10;

    fn sum(&self, x: u32, y: u32) -> String {
        format!("{}{}{x}{y}", self.value.to_uppercase(), self.extra)
    }
}

#[test]
fn group_impl_dispatches_to_the_states_group() {
    let a = Wrap::<A> { value: 1, extra: 2 };
    let b = Wrap::<B> { value: 1, extra: 2 };
    let c = Wrap::<C> {
        value: "hi".into(),
        extra: '!',
    };

    assert_eq!(
        (a.sum(3, 4), b.sum(3, 4), c.sum(3, 4)),
        ("10".into(), "10".into(), "HI!34".into())
    );
    assert_eq!((b.tag(), c.tag()), ("numbers", "default"));
    assert_eq!((a.doubled(3), c.doubled(3)), ("9".into(), "HI!33".into()));
}

#[test]
fn group_impl_consts_reach_the_trait() {
    assert_eq!(<Wrap<A> as describe>::LIMIT, 42);
    assert_eq!(<Wrap<C> as describe>::LIMIT, 10);
}

#[typestate(state = S)]
struct Pair<S: Meta, I> {
    value: S::Value,
    extra: I,
}

#[group_trait(by = Meta)]
trait Total {
    fn total(&self) -> u32;
}

// The `Item = u32` bindings bound `I`, not the state, so `#[group_impl]`
// keeps them, inline and in the `where` clause.
#[group_impl(Numbers, state = S)]
impl<S: Meta, I: Iterator<Item = u32> + Clone> Total for Pair<S, I>
where
    I: ExactSizeIterator<Item = u32>,
{
    fn total(&self) -> u32 {
        self.value + self.extra.clone().sum::<u32>()
    }
}

#[test]
fn group_impl_keeps_bindings_on_other_parameters() {
    let pair = Pair::<A, _> {
        value: 1,
        extra: [2, 3].into_iter(),
    };
    assert_eq!(pair.total(), 6);
}

#[group_trait(by = Meta)]
trait Bump {
    fn bumped(&self) -> u32;
}

// A `where` clause that implies `Wrap<S>: WithState`, as a
// `CastableState` bound does through its supertraits, still leaves
// `S::Value` as `u32`.
#[group_impl(Numbers)]
impl<S: Meta> Bump for Wrap<S>
where
    Wrap<S>: WithState,
{
    fn bumped(&self) -> u32 {
        self.value + 1
    }
}

#[test]
fn group_impl_keeps_group_types_under_a_with_state_bound() {
    assert_eq!(Wrap::<A> { value: 1, extra: 2 }.bumped(), 2);
}

#[state_types]
trait Slot {
    type Bits;
}

#[state]
struct Empty;
#[state]
struct Taken;

#[group(Slots)]
impl Slot for (Empty, Taken) {
    #[size(4)]
    type Bits = u32;
}

#[typestate(unsafe_transmute = true)]
struct Slotted<S: Slot> {
    bits: S::Bits,
}

#[group_trait(by = Slot)]
trait Retag<To: Slot> {
    fn retagged(&mut self) -> &mut Slotted<To>;
}

// No `CastableState` bound, because `To` sharing the group of `S` already
// proves both states have the same layout.
#[group_impl(Slots, state = S)]
impl<S: Slot, To: SlotGroupMember<S::Marker>> Retag<To> for Slotted<S> {
    fn retagged(&mut self) -> &mut Slotted<To> {
        self.cast_state_mut::<To>()
    }
}

#[test]
fn group_impl_casts_to_a_state_of_its_group_without_a_bound() {
    let mut empty = Slotted::<Empty> { bits: 7 };
    let taken: &mut Slotted<Taken> = empty.retagged();
    assert_eq!(taken.bits, 7);
}

// --- `state = Type` implements the one state two groups share ---

mod concrete {
    use typestate_groups::{MorphFrom, Morphic};
    use typestate_groups_macros::{
        group, group_impl, group_trait, state, state_types, typestate,
    };

    #[state_types]
    pub trait Kind {
        type Tag;
    }

    #[state_types]
    pub trait Place {}

    #[expect(
        dead_code,
        reason = "`Head` only gives `InList` a second state"
    )]
    #[state]
    pub struct Head;
    #[state]
    pub struct Linked;
    #[state]
    pub struct Loose;

    #[group(Heads)]
    impl Kind for (Head,) {
        type Tag = ();
    }

    #[group(Nodes)]
    impl Kind for (Linked, Loose) {
        #[size(4)]
        type Tag = u32;
    }

    #[group(InList)]
    impl Place for (Head, Linked) {}

    #[group(OutOfList)]
    impl Place for (Loose,) {}

    #[typestate(unsafe_transmute = true)]
    pub struct Node<S: Kind> {
        pub tag: S::Tag,
    }

    impl MorphFrom<Node<Linked>> for Node<Loose> {
        fn morph_from(src: Node<Linked>) -> Self {
            Node { tag: src.tag + 1 }
        }
    }

    #[group_trait(by = (Kind, Place))]
    pub trait Leave {
        fn leave(&mut self) -> &mut Node<Loose>;
    }

    // `Nodes` and `InList` share only `Linked`. Naming it lets `morph_mut`
    // find `MorphFrom<Node<Linked>>` without a `where` clause.
    #[group_impl(Kind = Nodes, Place = InList, state = Linked)]
    impl Leave for Node<Linked> {
        fn leave(&mut self) -> &mut Node<Loose> {
            self.morph_mut::<Loose>()
        }
    }

    #[typestate(state = S, unsafe_transmute = true)]
    pub struct Item<T, S: Kind> {
        pub value: T,
        pub tag: S::Tag,
    }

    impl<T> MorphFrom<Item<T, Linked>> for Item<T, Loose> {
        fn morph_from(src: Item<T, Linked>) -> Self {
            Item {
                value: src.value,
                tag: src.tag + 1,
            }
        }
    }

    #[group_trait(by = (Kind, Place))]
    pub trait LeaveItem<T> {
        fn leave(&mut self) -> &mut Item<T, Loose>;
    }

    // The impl's other type parameter stays generic.
    #[group_impl(Kind = Nodes, Place = InList, state = Linked)]
    impl<T> LeaveItem<T> for Item<T, Linked> {
        fn leave(&mut self) -> &mut Item<T, Loose> {
            self.morph_mut::<Loose>()
        }
    }
}

#[test]
fn group_impl_on_a_concrete_state_morphs_without_a_bound() {
    use concrete::{Leave, Linked, Node};

    let mut node = Node::<Linked> { tag: 1 };
    assert_eq!(node.leave().tag, 2);
}

#[test]
fn group_impl_on_a_concrete_state_keeps_other_parameters_generic() {
    use concrete::{Item, LeaveItem, Linked};

    let mut item = Item::<u16, Linked> { value: 7, tag: 1 };
    let loose = item.leave();
    assert_eq!((loose.value, loose.tag), (7, 2));
}
