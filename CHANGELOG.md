# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0](https://github.com/sagi21805/typestate-groups/compare/typestate-groups-v0.2.2...typestate-groups-v0.3.0) - 2026-10-06

### Fixed

- fix error message

### Other

- made test compile fail as intended
- update README
- Made code readable, and more simplified.
- started simlifying trait system
- replace pointer syntax matching with the Indirect trait and rewrite the readme as a guide
- allow pointer fields in transmutable typestates and check their pointees in casts
- pinned rust version to make errors the same on CI
- formatting
- matched new names
- chagnes to tests because of code changes
- optimized some search routines so they can early stop
- refactored typestate, mainly splitting into files
- replace let chain in outlives visitor to keep the 1.85 msrv

## [0.2.2](https://github.com/sagi21805/typestate-groups/compare/typestate-groups-v0.2.1...typestate-groups-v0.2.2) - 2026-10-01

### Other

- show the miri ub cases fail to compile with cast_state and split CastRefFrom from CastFrom
- document cast_state in the readme
- add CastRefFrom and point cast errors at morph
- add safe cast_state family checked with zerocopy
- move syn visitors out of function bodies
- copy where-clause bounds on the state onto the target state
- merge ui tests per macro and runtime tests per feature
- some fixes on claude code.
- run expected ub cases by hand instead of with a script
- add miri cast tests and expected ub checks for unchecked transmutes
- add char and NonZeroU32 cast tests for miri
- run miri in ci and skip ui tests under miri
- add miri test for transmute_state between u8 and bool states
- add cast_state runtime and ui tests

## [0.2.1](https://github.com/sagi21805/typestate-groups/compare/typestate-groups-v0.2.0...typestate-groups-v0.2.1) - 2026-10-01

### Other

- set readme install version to 0.2
- point repository links to typestate-groups

### Changed

- Renamed the crates from `groupoid` and `groupoid_macros` to
  `typestate-groups` and `typestate-groups-macros`. Import paths change
  from `groupoid::` to `typestate_groups::`.
- Renamed the `#[template]` attribute to `#[state_types]`.

## [0.2.0](https://github.com/sagi21805/typestate-groups/compare/typestate-groups-v0.1.1...typestate-groups-v0.2.0) - 2026-10-01

### Other

- Changed readme for new morph syntax
- Greatly simplified morphing.
- document several associated types, morph and the struct Morph in the readme and macro docs
- add the template Morph trait and morph to apply a reusable morpher
- check every field offset in LAYOUT_CHECK so align = N can't hide a moved field
- small modifications, mainly to decrease doc size.
- Changed trait and file structure to be much more readable, while preserving clear errors
- replace SizedGroup and SizedWithState with per-type layout markers checked by the target state
- allow several associated types in a template and morph each projection with its own closure
- Delete groupoid_macros/test.rs

### Removed

- removed unused struct in test to satisfy clippy

## [0.1.1](https://github.com/sagi21805/typestate-groups/compare/typestate-groups-v0.1.0...typestate-groups-v0.1.1) - 2026-09-26

### Other

- update readme
