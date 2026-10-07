// `cast_state` rejects every case where `tests/miri_ub.rs` finds undefined
// behaviour in `transmute_state`, under the same names, and states that
// can't transmute at all.
#![allow(dead_code)]

use core::{cell::Cell, num::NonZeroU32, ptr::NonNull};
use std::sync::Arc;
use typestate_groups::{CastableState, Isomorphic, ByValue};
use typestate_groups_macros::{group, state, state_types, typestate};

#[state_types]
trait Byte {
    type Value;
}

#[state]
struct Raw;
#[state]
struct Flag;
#[state]
struct Pair;

#[group(RawGroup)]
impl Byte for (Raw,) {
    #[size(1)]
    type Value = u8;
}

#[group(FlagGroup)]
impl Byte for (Flag,) {
    #[size(1)]
    type Value = bool;
}

#[group(PairGroup)]
impl Byte for (Pair,) {
    #[size(2)]
    type Value = [u8; 2];
}

#[typestate(unsafe_transmute = true)]
struct Small<S: Byte> {
    value: S::Value,
}

#[typestate]
struct Opaque<S: Byte> {
    value: S::Value,
}

#[state_types]
trait Word {
    type Value;
}

#[state]
struct Bits;
#[state]
struct Letter;
#[state]
struct Count;
#[state]
struct Shared;
#[state]
struct Gapped;
#[state]
struct Quad;

/// One padding byte after `small`.
#[derive(zerocopy::FromBytes)]
#[repr(C)]
struct Padded {
    small: u8,
    wide: u16,
}

#[group(BitsGroup)]
impl Word for (Bits,) {
    #[size(4)]
    type Value = u32;
}

#[group(LetterGroup)]
impl Word for (Letter,) {
    #[size(4)]
    type Value = char;
}

#[group(CountGroup)]
impl Word for (Count,) {
    #[size(4)]
    type Value = NonZeroU32;
}

#[group(SharedGroup)]
impl Word for (Shared,) {
    #[size(4)]
    type Value = Cell<u32>;
}

#[group(GappedGroup)]
impl Word for (Gapped,) {
    #[size(4)]
    type Value = Padded;
}

#[group(QuadGroup)]
impl Word for (Quad,) {
    #[size(4)]
    type Value = [u8; 4];
}

#[typestate(unsafe_transmute = true, align = 4)]
struct Large<S: Word> {
    value: S::Value,
}

#[typestate(unsafe_transmute = true)]
struct Pointer<S: Word> {
    value: NonNull<S::Value>,
}

#[typestate(unsafe_transmute = true)]
struct Ref<'a, S: Word> {
    value: &'a S::Value,
}

#[typestate(unsafe_transmute = true)]
struct Counted<S: Word> {
    value: Arc<S::Value>,
}

#[typestate(unsafe_transmute = true)]
struct RefMut<'a, S: Byte> {
    value: &'a mut S::Value,
}

fn transmute_state_u8_two_into_bool(raw: Small<Raw>) {
    let _ = raw.cast_state::<Flag>();
}

fn transmute_state_surrogate_into_char(bits: Large<Bits>) {
    let _ = bits.cast_state::<Letter>();
}

fn transmute_state_zero_into_non_zero(bits: &Large<Bits>) {
    let _ = bits.cast_state_ref::<Count>();
}

fn transmute_state_reads_padding_as_an_integer(gapped: Large<Gapped>) {
    let _ = gapped.cast_state::<Bits>();
}

fn transmute_state_mut_writes_an_invalid_bool_back(flag: &mut Small<Flag>) {
    flag.cast_state_mut::<Raw>().value = 2;
}

fn transmute_state_ref_writes_through_a_shared_integer(bits: &Large<Bits>) {
    bits.cast_state_ref::<Shared>().value.set(2);
}

fn shared_cell_into_integer(shared: &Large<Shared>) {
    let _ = shared.cast_state_ref::<Bits>();
}

fn transmute_state_non_null_surrogate_into_char(bits: Pointer<Bits>) {
    let _ = bits.cast_state::<Letter>();
}

fn transmute_state_shared_pointee_into_cell(bits: Ref<Bits>) {
    let _ = bits.cast_state::<Shared>();
}

fn arc_pointee_into_cell(bits: Counted<Bits>) {
    let _ = bits.cast_state::<Shared>();
}

fn transmute_state_borrowed_bool_gets_an_invalid_byte_back(flag: RefMut<Flag>) {
    let _ = flag.cast_state::<Raw>();
}

// `transmute_state_reads_a_misaligned_pointee`: a `[u8; 4]` pointee may
// sit where a `u32` can't. `cast_state` evaluates `POINTEE_CHECK` only
// under `cargo build`, and trybuild runs `cargo check`, so a const item
// forces it here.
const _: () = <Pointer<Quad> as CastableState<Bits, ByValue>>::POINTEE_CHECK;

fn size_mismatch(raw: Small<Raw>) {
    let _ = raw.cast_state::<Pair>();
}

fn without_unsafe_transmute(raw: Opaque<Raw>) {
    let _ = raw.cast_state::<Flag>();
}

#[typestate(unsafe_transmute = true)]
struct Linked<S: Byte> {
    value: S::Value,
    next: Option<NonNull<Linked<S>>>,
}

/// Points at a tuple holding its own struct, not at the struct.
#[typestate(unsafe_transmute = true)]
struct Tupled<S: Byte> {
    value: S::Value,
    next: Option<NonNull<(Tupled<S>, S::Value)>>,
}

#[typestate(unsafe_transmute = true)]
struct Holder<S: Byte> {
    small: Box<Small<S>>,
}

fn transmute_state_self_pointer_writes_an_invalid_bool(flag: Linked<Flag>) {
    let _ = flag.cast_state::<Raw>();
}

fn container_pointee_u8_two_into_bool(raw: Holder<Raw>) {
    let _ = raw.cast_state::<Flag>();
}

fn self_pointer_to_a_tuple(raw: Tupled<Raw>) {
    let _ = raw.cast_state::<Raw>();
}

/// Points at `Second`, which points back.
#[typestate(unsafe_transmute = true)]
struct First<S: Byte> {
    value: S::Value,
    second: Option<NonNull<Second<S>>>,
}

#[typestate(unsafe_transmute = true)]
struct Second<S: Byte> {
    value: S::Value,
    first: Option<NonNull<First<S>>>,
}

fn mutually_recursive_containers(raw: First<Raw>) {
    let _ = raw.cast_state::<Raw>();
}

fn main() {}
