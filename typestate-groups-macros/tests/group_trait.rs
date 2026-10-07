//! `#[group_trait]` on traits with generic parameters.

use typestate_groups::{Isomorphic, WithState};
use typestate_groups_macros::{
    group, group_impl, group_trait, state, state_types, typestate,
};

#[state_types]
trait Meta {
    type Value;
}

#[state_types]
trait List {
    type Head: Meta;
    type Attached: Meta;
    type Detached: Meta;
}

#[state]
struct FreeHead;
#[state]
struct FreeAttached;
#[state]
struct FreeDetached;
#[state]
struct FullHead;
#[state]
struct FullAttached;
#[state]
struct FullDetached;

#[group(Word)]
impl Meta
    for (
        FreeHead,
        FreeAttached,
        FreeDetached,
        FullHead,
        FullAttached,
        FullDetached,
    )
{
    #[size(4)]
    type Value = u32;
}

#[group(FreeList)]
impl List for (FreeHead, FreeAttached, FreeDetached) {
    type Head = FreeHead;
    type Attached = FreeAttached;
    type Detached = FreeDetached;
}

#[group(FullList)]
impl List for (FullHead, FullAttached, FullDetached) {
    type Head = FullHead;
    type Attached = FullAttached;
    type Detached = FullDetached;
}

#[typestate(state = S, unsafe_transmute = true)]
struct Node<T, S: Meta> {
    value: S::Value,
    tag: T,
}

#[group_trait(by = List)]
trait Attach<T>: WithState<State: List> {
    fn attach<'a>(
        &self,
        other: &'a mut Node<T, <Self::State as List>::Detached>,
    ) -> &'a mut Node<T, <Self::State as List>::Attached>;
}

#[group_impl(FreeList, state = S)]
impl<T, S: Meta + List> Attach<T> for Node<T, S> {
    fn attach<'a>(
        &self,
        other: &'a mut Node<T, S::Detached>,
    ) -> &'a mut Node<T, S::Attached> {
        let attached = other.cast_state_mut::<S::Attached>();
        attached.value += 1;
        attached
    }
}

#[group_impl(FullList, state = S)]
impl<T, S: Meta + List> Attach<T> for Node<T, S> {
    fn attach<'a>(
        &self,
        other: &'a mut Node<T, S::Detached>,
    ) -> &'a mut Node<T, S::Attached> {
        let attached = other.cast_state_mut::<S::Attached>();
        attached.value *= 2;
        attached
    }
}

#[test]
fn generic_group_trait_dispatches_to_the_states_group() {
    let free = Node::<char, FreeHead> { value: 3, tag: 'f' };
    let mut detached = Node::<char, FreeDetached> { value: 4, tag: 'd' };
    let attached: &mut Node<char, FreeAttached> =
        free.attach(&mut detached);
    assert_eq!((attached.value, attached.tag), (5, 'd'));

    let full = Node::<char, FullHead> { value: 3, tag: 'f' };
    let mut detached = Node::<char, FullDetached> { value: 4, tag: 'd' };
    let attached: &mut Node<char, FullAttached> =
        full.attach(&mut detached);
    assert_eq!(attached.value, 8);
}

#[group_trait(by = List)]
trait Repeat<'a, const N: usize> {
    fn repeat(&self, word: &'a str) -> [&'a str; N];
}

#[group_impl(FreeList)]
impl<'a, const N: usize, S: Meta> Repeat<'a, N> for Node<(), S> {
    fn repeat(&self, word: &'a str) -> [&'a str; N] {
        [word; N]
    }
}

#[test]
fn generic_group_trait_keeps_lifetime_and_const_parameters() {
    let node = Node::<(), FreeHead> { value: 0, tag: () };
    assert_eq!(Repeat::<2>::repeat(&node, "hi"), ["hi", "hi"]);
}

#[group_trait(by = List)]
trait Weigh {
    fn weigh(&self) -> u32;
}

// `List` is the grouped trait, so a binding on `Meta` stays.
#[group_impl(FreeList, state = S)]
impl<T, S> Weigh for Node<T, S>
where
    S: List + Meta<Value = u32>,
{
    fn weigh(&self) -> u32 {
        self.value + 1
    }
}

#[test]
fn group_impl_keeps_bindings_on_other_state_types() {
    let node = Node::<(), FreeAttached> { value: 4, tag: () };
    assert_eq!(node.weigh(), 5);
}
