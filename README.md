# typestate-groups

[![crates.io](https://img.shields.io/crates/v/typestate-groups.svg)](https://crates.io/crates/typestate-groups)
[![docs.rs](https://docs.rs/typestate-groups/badge.svg)](https://docs.rs/typestate-groups)
[![CI](https://github.com/sagi21805/typestate-groups/actions/workflows/ci.yml/badge.svg)](https://github.com/sagi21805/typestate-groups/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Typestate-based grouping types for Rust.

A typestate container changes its field types with its state. This crate
lets you group those states by the types they carry, write one trait impl
per group, and move a container between states by value or in place.

## Quick start

1. List the types that change with the state in a `#[state_types]` trait.
2. Declare each state with `#[state]`.
3. Group states with `#[group(Name)]` and set the types for the group.
4. Mark the container `#[typestate]`.

```rust
use typestate_groups::{group, state, state_types, typestate};

#[state_types]
trait Stage {
    type Sample;
}

#[state]
struct Sampled;
#[state]
struct Filtered;
#[state]
struct Calibrated;

// Raw ADC counts before calibration, volts after.
#[group(Counts)]
impl Stage for (Sampled, Filtered) {
    type Sample = u16;
}

#[group(Volts)]
impl Stage for (Calibrated,) {
    type Sample = f32;
}

#[typestate]
struct Frame<S: Stage> {
    sensor: u32,
    sample: S::Sample,
}

fn main() {
    let raw = Frame::<Sampled> { sensor: 7, sample: 4095 };
    let volts = Frame::<Calibrated> { sensor: 7, sample: 3.3 };

    assert_eq!(raw.sample, 4095u16);
    assert_eq!(volts.sample, 3.3f32);
}
```

`Frame<Sampled>` holds a `u16` and `Frame<Calibrated>` holds an `f32`.

## One impl per group

Declare a trait with `#[group_trait(by = Stage)]` and implement it once
per group with `#[group_impl(Group)]`. Every state in the group gets the
impl, and the impl sees the group's concrete types:

```rust
use typestate_groups::{group, group_impl, group_trait, state, state_types, typestate};

#[state_types]
trait Stage {
    type Sample;
}

#[state]
struct Sampled;
#[state]
struct Filtered;
#[state]
struct Calibrated;

#[group(Counts)]
impl Stage for (Sampled, Filtered) {
    type Sample = u16;
}

#[group(Volts)]
impl Stage for (Calibrated,) {
    type Sample = f32;
}

#[typestate]
struct Frame<S: Stage> {
    sensor: u32,
    sample: S::Sample,
}

#[group_trait(by = Stage)]
trait Report {
    fn report(&self) -> String;
}

#[group_impl(Counts)]
impl<S: Stage> Report for Frame<S> {
    fn report(&self) -> String {
        format!("sensor {}: {} counts", self.sensor, self.sample)
    }
}

#[group_impl(Volts)]
impl<S: Stage> Report for Frame<S> {
    fn report(&self) -> String {
        format!("sensor {}: {:.2} V", self.sensor, self.sample)
    }
}

fn main() {
    let filtered = Frame::<Filtered> { sensor: 7, sample: 12 };
    assert_eq!(filtered.report(), "sensor 7: 12 counts");

    let calibrated = Frame::<Calibrated> { sensor: 7, sample: 3.3 };
    assert_eq!(calibrated.report(), "sensor 7: 3.30 V");
}
```

Plain Rust rejects this pair with `E0119: conflicting implementations`,
because coherence ignores associated types:

```rust,ignore
impl<S: Stage<Sample = u16>> Report for Frame<S> { .. }
impl<S: Stage<Sample = f32>> Report for Frame<S> { .. }
```

Add a state to a group's tuple and it gets `report()` with no new code.

## Changing state by value

Implement `MorphFrom` to say how one state becomes another, then call
`morph::<Target>()`. For a conversion that can fail, implement
`TryMorphFrom` and call `try_morph::<Target>()`:

```rust
use typestate_groups::{
    MorphFrom, Morphic, TryMorphFrom, group, state, state_types, typestate,
};

#[state_types]
trait Stage {
    type Sample;
}

#[state]
struct Sampled;
#[state]
struct Filtered;
#[state]
struct Calibrated;

#[group(Counts)]
impl Stage for (Sampled, Filtered) {
    type Sample = u16;
}

#[group(Volts)]
impl Stage for (Calibrated,) {
    type Sample = f32;
}

#[typestate]
struct Frame<S: Stage> {
    sensor: u32,
    sample: S::Sample,
}

// A saturated reading can't be filtered.
impl TryMorphFrom<Frame<Sampled>> for Frame<Filtered> {
    type Error = u16;

    fn try_morph_from(src: Frame<Sampled>) -> Result<Self, u16> {
        match src.sample {
            4095 => Err(src.sample),
            sample => Ok(Frame { sensor: src.sensor, sample }),
        }
    }
}

impl MorphFrom<Frame<Filtered>> for Frame<Calibrated> {
    fn morph_from(src: Frame<Filtered>) -> Self {
        Frame {
            sensor: src.sensor,
            sample: f32::from(src.sample) * 3.3 / 4095.0,
        }
    }
}

fn main() {
    let saturated = Frame::<Sampled> { sensor: 7, sample: 4095 };
    assert_eq!(saturated.try_morph::<Filtered>().err(), Some(4095));

    let frame = Frame::<Sampled> { sensor: 7, sample: 4095 / 2 };
    let volts = frame.try_morph::<Filtered>().unwrap().morph::<Calibrated>();
    assert!((volts.sample - 1.65).abs() < 0.01);
}
```

One impl can be generic over both states, such as
`impl<S: Stage, S2: Stage> MorphFrom<Frame<S>> for Frame<S2> where
S2::Sample: From<S::Sample>`.

## Reinterpreting in place

When two states hold types of the same size, you can reinterpret the
container's bits instead of converting field by field:

1. Add `#[size(N)]` to each associated type in the groups you want to
   reinterpret between.
2. Add `unsafe_transmute = true` to `#[typestate]`. It adds
   `#[repr(C)]`, and `align = N` raises the container's alignment when
   the states' types disagree on it.
3. Call `cast_state::<Target>()`, or its `_ref` and `_mut` forms.

```rust
use typestate_groups::{Isomorphic, group, state, state_types, typestate};

#[state_types]
trait Wire {
    type Addr;
}

#[state]
struct Received;
#[state]
struct Routed;

#[group(Bytes)]
impl Wire for (Received,) {
    #[size(4)]
    type Addr = [u8; 4];
}

#[group(Native)]
impl Wire for (Routed,) {
    #[size(4)]
    type Addr = u32;
}

// `[u8; 4]` is 1-aligned and `u32` is 4-aligned.
#[typestate(unsafe_transmute = true, align = 4)]
struct Header<S: Wire> {
    dst: S::Addr,
    ttl: u8,
}

fn main() {
    let header = Header::<Received> { dst: [10, 0, 0, 2], ttl: 64 };
    let routed = header.cast_state::<Routed>();

    assert_eq!(routed.dst, u32::from_ne_bytes([10, 0, 0, 2]));
    assert_eq!(routed.ttl, 64);
}
```

The size and layout checks run at compile time. Give `Routed` a `u64`
address and the build fails asking you to fix `#[size(4)]`.

`cast_state` also checks that every field that changes type holds bits
valid in the new state, using [`zerocopy`](https://docs.rs/zerocopy):
the old type must be `IntoBytes` (no padding) and the new one `FromBytes`
(every bit pattern valid). Casting a `u8` into a `bool` fails with
``convert with `morph`: `u8` may hold bits that are not a valid `bool` ``.

| Method | Each changed field also needs |
|---|---|
| `cast_state` | nothing more |
| `cast_state_mut` | the same check from the new type back to the old |
| `cast_state_ref` | `zerocopy::Immutable` on both types, so no `Cell` |

When a type is valid only for some bit patterns, such as `u32` into
`char`, call `unsafe { transmute_state() }` and run `cargo miri test` on
the code that calls it.

## Pointer fields

A reinterpreted container can also hold the state's types behind a
pointer. A pointer to a sized type has the same layout whatever it
points at, so the pointee needs no `#[size(N)]`. The field's type must
implement `Indirect`, which the crate implements for:

- `*const T`, `*mut T`, `NonNull<T>`, `&T`, `&mut T`
- `Box<T>`, `Arc<T>`, `Rc<T>`, with the default `alloc` feature
- `Option<P>` when `P` is one of the non-null pointers above

```rust
use typestate_groups::{Isomorphic, group, state, state_types, typestate};

#[state_types]
trait Encoding {
    type Word;
}

#[state]
struct Unsigned;
#[state]
struct Signed;

#[group(UnsignedGroup)]
impl Encoding for (Unsigned,) {
    type Word = u32;
}

#[group(SignedGroup)]
impl Encoding for (Signed,) {
    type Word = i32;
}

#[typestate(unsafe_transmute = true)]
struct Buffer<'a, S: Encoding> {
    owned: Box<S::Word>,
    borrowed: Option<&'a S::Word>,
}

fn main() {
    let word = u32::MAX;
    let unsigned = Buffer::<Unsigned> {
        owned: Box::new(u32::MAX),
        borrowed: Some(&word),
    };
    let signed = unsigned.cast_state::<Signed>();

    assert_eq!(*signed.owned, -1);
    assert_eq!(signed.borrowed, Some(&-1));
}
```

`cast_state` checks a pointee as if the container held it by value, and
the pointee must keep its size and alignment. Others may see a pointee
through the pointer, so some pointers need the stricter checks:

| Pointer | Pointee also needs |
|---|---|
| `Box<T>` | nothing more |
| `&T`, `Arc<T>`, `Rc<T>` | the `cast_state_ref` check |
| `&mut T` | the `cast_state_mut` check, except under `cast_state_ref` |
| `*const T`, `*mut T`, `NonNull<T>` | both |

`Vec` and `Result` don't implement `Indirect`. `Result<T, E>` stores `T`
inline, and the compiler packs its tag into `T`'s invalid bit patterns:
`Result<char, ()>` is 4 bytes while `Result<u32, ()>` is 8. `Vec<T>`
holds a pointer, a length and a capacity, and Rust doesn't promise the
same field order for every `T`.

## Implementing the crate's traits on your own types

### A pointer of your own

Implement `Indirect` and `Repointed` to use your own pointer type as a
field:

```rust
use core::ptr::NonNull;
use typestate_groups::{Indirect, Repointed, UnknownPointee};

#[repr(transparent)]
struct Handle<T>(NonNull<T>);

// SAFETY: `Handle` holds `T` only behind its `NonNull`, which anyone may
// alias.
unsafe impl<T> Indirect for Handle<T> {
    type Pointee = T;
    type Aliasing = UnknownPointee;
}

// SAFETY: `Handle<U>` is a `NonNull<U>`, laid out like `NonNull<T>`.
unsafe impl<T, U> Repointed<Handle<T>> for Handle<U> {}
```

Both traits are `unsafe` because the casts trust them:

- `Indirect` promises that the type holds no `Pointee` inline, and that
  `Aliasing` covers everyone who may see the pointee. Pick
  `UniquePointee` if only your type reaches it, `SharedPointee` if others
  may read it, `LentPointee` if it goes back to a lender, and
  `UnknownPointee` otherwise.
- `Repointed<Src>` promises that `Self` is `Src` pointing at another
  type, with the same size, alignment and field layout.

Implement `NullNiche` too if your type is never null and `Option` of it
keeps its layout. That makes `Option<YourPointer<T>>` a valid field.

### Field types that cast

`cast_state` accepts any field type that implements zerocopy's traits.
Derive them on your own types:

```rust
use zerocopy::{FromBytes, Immutable, IntoBytes};

#[derive(FromBytes, IntoBytes, Immutable)]
#[repr(C)]
struct Rgba {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}
```

`IntoBytes` lets a cast read the type, `FromBytes` lets a cast produce it,
and `Immutable` allows `cast_state_ref`.

### Conversions

`MorphFrom` and `TryMorphFrom` are ordinary traits you implement for each
pair of states, as in [Changing state by value](#changing-state-by-value).

### What `#[typestate]` implements for you

`#[typestate]` implements `WithState` and `Restate`, and with
`unsafe_transmute = true` also `TransmutableState` and `CastableState`.
Don't implement these by hand: their safety depends on the layout checks
the macro generates.

## Crates

- [`typestate-groups`](typestate-groups): the public API.
- [`typestate-groups-macros`](typestate-groups-macros): the procedural
  macros behind it.

## Installation

```toml
[dependencies]
typestate-groups = "0.2"
```

The crate is `no_std`. Its default `alloc` feature adds the `Box`, `Arc`
and `Rc` impls; turn it off with `default-features = false`.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in this crate by you, as defined in the Apache-2.0 license, shall be
dual-licensed as above, without any additional terms or conditions.
