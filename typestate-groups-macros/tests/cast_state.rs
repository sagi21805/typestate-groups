//! `cast_state` and its ref/mut forms between states whose fields stay
//! valid. Runs under `cargo +nightly miri test` too.

use core::{
    cell::Cell, marker::PhantomData, num::NonZeroU32, ptr::NonNull,
};
use std::{rc::Rc, sync::Arc};
use typestate_groups::Isomorphic;
use typestate_groups_macros::{group, state, state_types, typestate};
use zerocopy::{FromBytes, Immutable, IntoBytes};

#[state_types]
trait Meta {
    type Value;
    type Extra;
}

#[state]
struct Unsigned;
#[state]
struct Signed;
#[state]
struct Pixel;
#[state]
struct Letter;
#[state]
struct Count;

#[derive(FromBytes, IntoBytes, Immutable)]
#[repr(C)]
struct Rgba {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

#[group(UnsignedGroup)]
impl Meta for (Unsigned,) {
    #[size(4)]
    type Value = u32;
    #[size(2)]
    type Extra = [u8; 2];
}

#[group(SignedGroup)]
impl Meta for (Signed,) {
    #[size(4)]
    type Value = i32;
    #[size(2)]
    type Extra = u16;
}

#[group(PixelGroup)]
impl Meta for (Pixel,) {
    #[size(4)]
    type Value = Rgba;
    #[size(2)]
    type Extra = i16;
}

#[group(LetterGroup)]
impl Meta for (Letter,) {
    #[size(4)]
    type Value = char;
    #[size(2)]
    type Extra = [u8; 2];
}

#[group(CountGroup)]
impl Meta for (Count,) {
    #[size(4)]
    type Value = NonZeroU32;
    #[size(2)]
    type Extra = [u8; 2];
}

/// Padding sits between `extra` and an 8-aligned `payload`.
#[typestate(state = S, unsafe_transmute = true, align = 8)]
struct Wrap<S: Meta, T> {
    value: S::Value,
    extra: S::Extra,
    payload: T,
    _state: PhantomData<S>,
}

fn wrap<S: Meta, T>(
    value: S::Value,
    extra: S::Extra,
    payload: T,
) -> Wrap<S, T> {
    Wrap {
        value,
        extra,
        payload,
        _state: PhantomData,
    }
}

#[test]
fn cast_state_reinterprets_every_projection() {
    let unsigned =
        wrap::<Unsigned, _>(u32::MAX, [0x34, 0x12], String::from("owned"));
    let signed = unsigned.cast_state::<Signed>();
    assert_eq!(signed.value, -1);
    assert_eq!(signed.extra, u16::from_ne_bytes([0x34, 0x12]));
    assert_eq!(signed.payload, "owned");

    let rgba = Rgba {
        r: 1,
        g: 2,
        b: 3,
        a: 4,
    };
    let unsigned = wrap::<Pixel, _>(rgba, -1, Box::new(5u64))
        .cast_state::<Unsigned>();
    assert_eq!(unsigned.value, u32::from_ne_bytes([1, 2, 3, 4]));
    assert_eq!(unsigned.extra, [0xff, 0xff]);
    assert_eq!(*unsigned.payload, 5);
}

#[test]
fn cast_state_out_of_types_with_invalid_bit_patterns() {
    let letter = wrap::<Letter, _>('z', [0, 0], ());
    assert_eq!(letter.cast_state::<Unsigned>().value, 'z' as u32);

    let count = wrap::<Count, _>(NonZeroU32::MIN, [0, 0], ());
    assert_eq!(count.cast_state_ref::<Unsigned>().value, 1);
}

#[test]
fn cast_state_ref_interleaves_with_other_shared_borrows() {
    let signed = wrap::<Signed, _>(-2, 0, vec![1u8, 2]);
    let plain = &signed;
    let unsigned = signed.cast_state_ref::<Unsigned>();
    let pixel = signed.cast_state_ref::<Pixel>();

    assert_eq!(unsigned.value, u32::MAX - 1);
    assert_eq!(plain.value, -2);
    assert_eq!(pixel.value.r, 0xfe);
    assert_eq!(unsigned.payload, plain.payload);
}

#[test]
fn cast_state_mut_writes_are_seen_after_the_borrow_ends() {
    let mut unsigned = wrap::<Unsigned, _>(0, [0, 0], String::new());

    {
        let pixel = unsigned.cast_state_mut::<Pixel>();
        pixel.value.a = 0x80;
        pixel.payload.push_str("written");
    }
    assert_eq!(unsigned.value, u32::from_ne_bytes([0, 0, 0, 0x80]));
    assert_eq!(unsigned.payload, "written");

    let signed = unsigned.cast_state_mut::<Signed>();
    signed.cast_state_mut::<Pixel>().value.r = 8;
    assert_eq!(unsigned.value.to_ne_bytes()[0], 8);
}

/// Every pointer kind, whose pointees cast like fields held by value.
#[typestate(unsafe_transmute = true)]
struct Pointers<'a, S: Meta> {
    raw: *mut S::Value,
    nullable: Option<NonNull<S::Value>>,
    shared: &'a S::Value,
    unique: &'a mut S::Value,
    boxed: Box<S::Value>,
    atomic: Arc<S::Value>,
    counted: Option<Rc<S::Value>>,
}

#[test]
fn cast_state_reinterprets_every_pointee() {
    let mut target = u32::MAX;
    let ptr = &raw mut target;
    let shared = 1;
    let mut unique = 2;
    let unsigned = Pointers::<Unsigned> {
        raw: ptr,
        nullable: NonNull::new(ptr),
        shared: &shared,
        unique: &mut unique,
        boxed: Box::new(3),
        atomic: Arc::new(u32::MAX),
        counted: Some(Rc::new(5)),
    };
    let atomic = Arc::clone(&unsigned.atomic);

    let view = unsigned.cast_state_ref::<Signed>();
    assert_eq!((*view.shared, *view.boxed), (1, 3));
    assert_eq!(view.counted.as_deref(), Some(&5));

    let mut signed = unsigned.cast_state::<Signed>();
    *signed.unique = -1;
    *signed.cast_state_mut::<Unsigned>().boxed = 4;
    assert_eq!(*signed.boxed, 4);
    // SAFETY: `ptr` points at `target`, which outlives `signed`.
    unsafe {
        assert_eq!(*signed.raw, -1);
        assert_eq!(signed.nullable.map(|p| *p.as_ptr()), Some(-1));
    }

    assert_eq!(*signed.atomic, -1);

    drop(signed);
    assert_eq!(unique, u32::MAX);
    assert_eq!(Arc::strong_count(&atomic), 1);
}

#[state_types]
trait Flagged {
    type Value;
}

#[state]
struct Flag;
#[state]
struct Byte;
#[state]
struct Shared;

#[group(FlagGroup)]
impl Flagged for (Flag,) {
    #[size(1)]
    type Value = bool;
}

#[group(ByteGroup)]
impl Flagged for (Byte,) {
    #[size(1)]
    type Value = u8;
}

#[group(SharedGroup)]
impl Flagged for (Shared,) {
    #[size(1)]
    type Value = Cell<u8>;
}

#[typestate(unsafe_transmute = true)]
struct Slot<S: Flagged> {
    value: S::Value,
}

#[test]
fn cast_state_turns_bools_and_cells_into_bytes() {
    let flag = Slot::<Flag> { value: true };
    assert_eq!(flag.cast_state_ref::<Byte>().value, 1);
    assert_eq!(flag.cast_state::<Byte>().value, 1);

    let mut shared = Slot::<Shared> {
        value: Cell::new(3),
    };
    shared.cast_state_mut::<Byte>().value = 4;
    assert_eq!(shared.value.get(), 4);
    assert_eq!(shared.cast_state::<Byte>().value, 4);
}

/// A node of an intrusive list, which points at its own struct.
#[typestate(unsafe_transmute = true)]
struct Node<S: Meta> {
    value: S::Value,
    next: Option<NonNull<Node<S>>>,
}

/// Links `a -> b -> c -> a` on the heap and returns the three nodes.
fn cyclic_list() -> [*mut Node<Unsigned>; 3] {
    let node = |value| {
        Box::into_raw(Box::new(Node::<Unsigned> { value, next: None }))
    };
    let nodes = [node(u32::MAX), node(2), node(3)];
    for (from, to) in nodes.iter().zip(nodes.iter().cycle().skip(1)) {
        // SAFETY: every node comes from `Box::into_raw` and is still
        // live.
        unsafe { (**from).next = NonNull::new(*to) };
    }
    nodes
}

/// Frees nodes from `cyclic_list`.
fn free_list(nodes: [*mut Node<Unsigned>; 3]) {
    for node in nodes {
        // SAFETY: `cyclic_list` made `node` with `Box::into_raw`, and no
        // reference to it is live.
        drop(unsafe { Box::from_raw(node) });
    }
}

/// The values met walking `count` nodes from `node`.
fn walk(mut node: &Node<Signed>, count: usize) -> Vec<i32> {
    let mut values = Vec::new();
    for _ in 0..count {
        values.push(node.value);
        // SAFETY: the list is cyclic, and every node is live.
        node = unsafe { node.next.expect("the list is cyclic").as_ref() };
    }
    values
}

#[test]
fn cast_state_follows_an_intrusive_cyclic_list() {
    let nodes = cyclic_list();
    let [a, b, _] = nodes;

    // SAFETY: `a` is live, and nothing writes the list during the walk.
    let head = unsafe { &*a }.cast_state_ref::<Signed>();
    assert_eq!(walk(head, 4), [-1, 2, 3, -1]);

    {
        // SAFETY: `a` is live, and no other reference to it is.
        let head = unsafe { &mut *a }.cast_state_mut::<Signed>();
        head.value = -2;
        let next = head.next.expect("the list is cyclic");
        // SAFETY: `next` is `b`, which is live and not borrowed.
        unsafe { (*next.as_ptr()).value = -5 };
    }
    // SAFETY: `a` and `b` are live, and no reference to them is.
    unsafe {
        assert_eq!(((*a).value, (*b).value), (u32::MAX - 1, u32::MAX - 4))
    };

    let outside = Node::<Unsigned> {
        value: 7,
        next: NonNull::new(a),
    };
    let outside = outside.cast_state::<Signed>();
    assert_eq!(walk(&outside, 5), [7, -2, -5, 3, -2]);

    free_list(nodes);
}

/// Holds another container behind a pointer.
#[typestate(unsafe_transmute = true)]
struct Tree<S: Meta> {
    leaf: Box<Leaf<S>>,
    raw: *mut Leaf<S>,
}

#[typestate(unsafe_transmute = true, align = 4)]
struct Leaf<S: Meta> {
    value: S::Value,
    extra: S::Extra,
}

#[test]
fn cast_state_reinterprets_containers_behind_pointers() {
    let shared = Box::into_raw(Box::new(Leaf::<Unsigned> {
        value: u32::MAX,
        extra: [1, 0],
    }));
    let unsigned = Tree::<Unsigned> {
        leaf: Box::new(Leaf {
            value: u32::MAX - 1,
            extra: [2, 0],
        }),
        raw: shared,
    };

    let view = unsigned.cast_state_ref::<Signed>();
    assert_eq!(view.leaf.value, -2);

    let mut signed = unsigned.cast_state::<Signed>();
    signed.cast_state_mut::<Unsigned>().leaf.value = 5;
    assert_eq!(signed.leaf.value, 5);
    // SAFETY: `shared` is live, and no reference to it is.
    unsafe {
        assert_eq!((*signed.raw).value, -1);
        (*signed.raw).extra = 3;
    }

    drop(signed);
    // SAFETY: `shared` came from `Box::into_raw`, and no reference to it
    // is live.
    let shared = unsafe { Box::from_raw(shared) };
    assert_eq!(shared.extra, 3u16.to_ne_bytes());
}
