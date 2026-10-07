# typestate-groups

[![crates.io](https://img.shields.io/crates/v/typestate-groups.svg)](https://crates.io/crates/typestate-groups)
[![docs.rs](https://docs.rs/typestate-groups/badge.svg)](https://docs.rs/typestate-groups)
[![CI](https://github.com/sagi21805/typestate-groups/actions/workflows/ci.yml/badge.svg)](https://github.com/sagi21805/typestate-groups/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Typestate-based grouping types for Rust.

A typestate container changes its field types with its state. This crate
groups states by the types they carry, so you write one trait impl per
group, and moves a container between states by value or in place.

```rust
use typestate_groups::{
    group, group_impl, group_trait, state, state_types, typestate,
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

// Raw ADC counts before calibration, volts after.
#[group(Counts)]
impl Stage for (Sampled, Filtered) {
    type Sample = u32;
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

## Guide

Each step below adds to the example above.

### 1. States and groups

- `#[state_types]` lists the types that change with the state. It can
  also hold consts, which a group sets or overrides for all its states.
- `#[state]` declares a state.
- `#[group(Name)]` puts a tuple of states in a group and sets their types.
- `#[typestate]` marks the container.

`Frame<Sampled>` holds a `u32` and `Frame<Calibrated>` holds an `f32`.

### 2. One impl per group

`#[group_trait(by = Stage)]` declares a trait that you implement once per
group with `#[group_impl(Group)]`. Every state in the group gets the
impl, so adding a state to `(Sampled, Filtered)` gives it `report()` with
no new code.

The trait can take type, lifetime and const parameters. Each group impl
names them as in any trait impl, for example
`#[group_impl(Counts, state = S)] impl<S: Stage, T> Scale<T> for Frame<S>`.

Plain Rust rejects this pair with `E0119: conflicting implementations`,
because coherence ignores associated types:

```rust,ignore
impl<S: Stage<Sample = u32>> Report for Frame<S> { .. }
impl<S: Stage<Sample = f32>> Report for Frame<S> { .. }
```

### 3. Changing state by value

Implement `MorphFrom` to say how one state becomes another, then call
`morph::<Target>()`. For a conversion that can fail, implement
`TryMorphFrom` and call `try_morph::<Target>()`:

```rust,ignore
use typestate_groups::{MorphFrom, Morphic, TryMorphFrom};

// A saturated reading can't be filtered.
impl TryMorphFrom<Frame<Sampled>> for Frame<Filtered> {
    type Error = u32;

    fn try_morph_from(src: Frame<Sampled>) -> Result<Self, u32> {
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
            sample: src.sample as f32 * 3.3 / 4095.0,
        }
    }
}

let frame = Frame::<Sampled> { sensor: 7, sample: 2047 };
let volts = frame.try_morph::<Filtered>()?.morph::<Calibrated>();
```

One impl can cover many pairs, such as `impl<S: Stage, S2: Stage>
MorphFrom<Frame<S>> for Frame<S2> where S2::Sample: From<S::Sample>`.

### 4. Reinterpreting in place

When two states hold types of the same size, you can reinterpret the
container's bits instead of converting each field. Add a `Centered` state
whose signed counts share the bits of `Filtered`'s:

```rust,ignore
use typestate_groups::Isomorphic;

#[state]
struct Centered;

#[group(SignedCounts)]
impl Stage for (Centered,) {
    #[size(4)]
    type Sample = i32;
}

// Also add `#[size(4)]` to `Sample` in `Counts` and `Volts`.

#[typestate(unsafe_transmute = true)]
struct Frame<S: Stage> {
    sensor: u32,
    sample: S::Sample,
}

let frame = Frame::<Filtered> { sensor: 7, sample: u32::MAX };
assert_eq!(frame.cast_state::<Centered>().sample, -1);
```

`unsafe_transmute = true` adds `#[repr(C)]`. When the states' types
disagree on alignment, such as `[u8; 4]` and `u32`, add `align = 4`.

The size and layout checks run at compile time. Give `Centered` an `i64`
and the build fails asking you to fix `#[size(4)]`.

`cast_state` also checks with [`zerocopy`](https://docs.rs/zerocopy) that
every field that changes type holds bits valid in the new state. The old
type must be `IntoBytes` (no padding) and the new one `FromBytes` (every
bit pattern valid). Casting a `u8` into a `bool` fails with
``convert with `morph`: `u8` may hold bits that are not a valid `bool` ``.

| Method | Each changed field also needs |
|---|---|
| `cast_state` | nothing more |
| `cast_state_mut` | the same check from the new type back to the old |
| `cast_state_ref` | `zerocopy::Immutable` on both types, so no `Cell` |

When a type is valid only for some bit patterns, such as `u32` into
`char`, call `unsafe { transmute_state() }` and run `cargo miri test` on
the code that calls it.

### 5. Pointer fields

A reinterpreted container can hold the state's types behind a pointer.
The pointee needs no `#[size(N)]`, because a pointer to a sized type has
the same layout whatever it points at:

```rust,ignore
#[typestate(unsafe_transmute = true)]
struct Window<'a, S: Stage> {
    latest: Box<S::Sample>,
    previous: Option<&'a S::Sample>,
}

let previous = u32::MAX;
let window = Window::<Filtered> {
    latest: Box::new(u32::MAX),
    previous: Some(&previous),
};
let centered = window.cast_state::<Centered>();
assert_eq!(centered.previous, Some(&-1));
```

The field's type must implement `Indirect`. The crate implements it for
`*const T`, `*mut T`, `NonNull<T>`, `&T` and `&mut T`, for `Box<T>`,
`Arc<T>` and `Rc<T>` with the default `alloc` feature, and for `Option<P>`
when `P` is one of the non-null pointers.

A pointee must keep its size and alignment. Others may see it through
the pointer, so each pointer names the access its pointee gets under
each cast method, and `Permits` checks that access with zerocopy's
traits:

| Access | Needs of `Src` and `Dst` |
|---|---|
| `Read` | `Src: IntoBytes`, `Dst: FromBytes` |
| `ReadShared` | `Read`, and both `Immutable` |
| `ReadWrite` | both `IntoBytes + FromBytes` |
| `ReadWriteShared` | `ReadWrite`, and both `Immutable` |

| Pointer | `cast_state` | `cast_state_ref` | `cast_state_mut` |
|---|---|---|---|
| `Box<T>`, or by value | `Read` | `ReadShared` | `ReadWrite` |
| `&T`, `Arc<T>`, `Rc<T>` | `ReadShared` | `ReadShared` | `ReadShared` |
| `&mut T` | `ReadWrite` | `ReadShared` | `ReadWrite` |
| `*const T`, `*mut T`, `NonNull<T>` | `ReadWriteShared` | `ReadWriteShared` | `ReadWriteShared` |

A pointer can also reach another `unsafe_transmute = true` container,
which `Permits` accepts whenever that container casts by the matching
method: `cast_state` for `Read`, `cast_state_ref` for `ReadShared`,
`cast_state_mut` for `ReadWrite`, and both of the last two for
`ReadWriteShared`. A container can point at itself, as in an intrusive
list:

```rust,ignore
#[typestate(unsafe_transmute = true)]
struct Node<S: Stage> {
    sample: S::Sample,
    next: Option<NonNull<Node<S>>>,
}
```

Every node the list reaches is cast too, so each projection must also
hold under the pointer's access, here `ReadWriteShared`. A container
that points at itself holds the state's types only by value. Two
containers that point at each other make rustc report E0275; convert
them with `morph`.

`Vec` and `Result` don't implement `Indirect`. `Result<T, E>` stores `T`
inline and packs its tag into `T`'s invalid bit patterns, so
`Result<char, ()>` is 4 bytes while `Result<u32, ()>` is 8. Rust doesn't
promise `Vec<T>` the same field order for every `T`.

### 6. Your own types

Derive zerocopy's traits to let your own types cast:

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

Implement `Indirect` and `Repointed` to use your own pointer as a field:

```rust
use core::ptr::NonNull;
use typestate_groups::{Indirect, ReadWriteShared, Repointed};

#[repr(transparent)]
struct Handle<T>(NonNull<T>);

// SAFETY: `Handle` holds `T` only behind its `NonNull`, which anyone may
// read or write through.
unsafe impl<T> Indirect for Handle<T> {
    type Pointee = T;
    type CastState = ReadWriteShared;
    type CastStateRef = ReadWriteShared;
    type CastStateMut = ReadWriteShared;
}

// SAFETY: `Handle<U>` is a `NonNull<U>`, laid out like `NonNull<T>`.
unsafe impl<T, U> Repointed<Handle<T>> for Handle<U> {}
```

Both traits are `unsafe` because the casts trust them. `Indirect`
promises the type holds no `Pointee` inline and that each access covers
everyone who may see the pointee. Copy the table row of the pointer yours
behaves like, or pick `ReadWriteShared` for all three. `Repointed<Src>`
promises `Self` is `Src` pointing at another type, with the same layout.
Implement `NullNiche` too if your pointer is never null, to allow
`Option<Handle<T>>`.

Don't implement `WithState`, `Restate`, `TransmutableState` or
`CastableState` by hand. `#[typestate]` implements them, and their safety
depends on the layout checks it generates.

### 7. Generic states

A state can take type, lifetime and const parameters. When a group's
types depend on one, the group lists it, and `#[group_impl]` names the
group the same way:

```rust
use core::marker::PhantomData;
use typestate_groups::{
    group, group_impl, group_trait, state, state_types, typestate,
};

#[state_types]
trait Stage {
    type Sample;
}

#[state]
struct Buffered<T>(PhantomData<fn() -> T>);

// `Sample` depends on `T`, so the group carries it.
#[group(Buffers<T>)]
impl<T: Copy> Stage for (Buffered<T>,) {
    type Sample = Vec<T>;
}

#[typestate]
struct Frame<S: Stage> {
    sample: S::Sample,
}

#[group_trait(by = Stage)]
trait Count {
    fn count(&self) -> usize;
}

#[group_impl(Buffers<T>, state = S)]
impl<T: Copy, S: Stage> Count for Frame<S> {
    fn count(&self) -> usize {
        self.sample.len()
    }
}

fn main() {
    let frame = Frame::<Buffered<u8>> { sample: vec![1, 2] };
    assert_eq!(frame.count(), 2);
}
```

Declare a generic state with `PhantomData<fn() -> T>`, which keeps it
`Send`, `Sync` and covariant whatever `T` is. A group can't mix
`Buffered<T>` with a state that lacks `T`, because rustc can't tell which
`T` the plain state means. `#[size(N)]` can't pin a type that depends on
a group parameter, so convert such states with `morph`.

A `#[state_types]` trait can take parameters too, such as
`trait Stage<T>` implemented with `impl<T> Stage<T> for (Sampled,)`. A
group whose types use `T` lists it in the same way. `#[group_trait]`
groups only by a trait without parameters.

## Installation

```toml
[dependencies]
typestate-groups = "0.x"
```

The crate is `no_std`. Its default `alloc` feature adds the `Box`, `Arc`
and `Rc` impls. Turn it off with `default-features = false`.

The procedural macros live in
[`typestate-groups-macros`](typestate-groups-macros).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in this crate by you, as defined in the Apache-2.0 license, shall be
dual-licensed as above, without any additional terms or conditions.
