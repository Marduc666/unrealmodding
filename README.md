# unrealmodding

[![Build status](https://github.com/AstroTechies/unrealmodding/workflows/CI/badge.svg)](https://github.com/AstroTechies/unrealmodding/actions?query=workflow%3ACI)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE-MIT)

Tools for creating and loading Unreal Engine Mods. Developed for Astroneer.

> This is a fork of [AstroTechies/unrealmodding](https://github.com/AstroTechies/unrealmodding) that builds on the
> [ASEAN Motor Club fork](https://github.com/ASEAN-Motor-Club/unrealmodding) and adds support for Unreal Engine 5.3 to
> 5.8 to `unreal_asset`, for the map editor [stove](https://github.com/Marduc666/stove). It adds:
>
> - engine versions 5.3 to 5.8 and the package summary fields added up to them
> - unversioned properties written the way the engine builds them, including enums, bitfield bools and weak,
>   interface and lazy object properties
> - the 5.8 changes to cooked imports, FField flags and soft object paths, including the expanded import layout some
>   5.8 games like The Last Caretaker use
> - blueprint schemas read from cooked packages when the usmap doesn't have them
> - the data resource table bulk data has used since 5.2
>
> `unreal_asset/tests/modern_engines.rs` reads and saves real assets. Point `UE_TEST_ASSETS` at UAssetAPI's test assets,
> or `UE_TEST_GAME` at a game extracted with [retoc](https://github.com/trumank/retoc) as
> `<folder>;<usmap>;<game>;<maps|all|blueprints>`, and run `cargo test --test modern_engines -- --nocapture`.

## Crates

This repo contrains multiple user facing crates for working with Unreal Engine file formats and creating Mods.

### [unreal_asset](./unreal_asset/)

[![Documentation](https://docs.rs/unreal_asset/badge.svg)](https://docs.rs/unreal_asset/)
[![Crates.io](https://img.shields.io/crates/v/unreal_asset.svg)](https://crates.io/crates/unreal_asset)

This core crate allows for parsing of Unreal asset binary files. It is internally split into multiple sub-crates to
improve compile times.

### [unreal_mod_manager](./unreal_mod_manager/)

[![Documentation](https://docs.rs/unreal_mod_manager/badge.svg)](https://docs.rs/unreal_mod_manager/)
[![Crates.io](https://img.shields.io/crates/v/unreal_mod_manager.svg)](https://crates.io/crates/unreal_mod_manager)

Crate that allows creating Modmanagers/Modloaders for individual games. Typically used together with
[unreal_mod_integrator](./unreal_mod_integrator/) and [unreal_asset](./unreal_asset/) (both reexported) to create
asset transformation logic for specific games.

### [unreal_pak](./unreal_pak/)

[![Documentation](https://docs.rs/unreal_pak/badge.svg)](https://docs.rs/unreal_pak/)
[![Crates.io](https://img.shields.io/crates/v/unreal_pak.svg)](https://crates.io/crates/unreal_pak)

Library crate for working with Unreal Engine .pak files. The CLI tool [unreal_pak_cli](./unreal_pak_cli/) is built on
this crate to provide a simple way to use this library.

### [unreal_helpers](./unreal_helpers/)

[![Documentation](https://docs.rs/unreal_helpers/badge.svg)](https://docs.rs/unreal_helpers/)
[![Crates.io](https://img.shields.io/crates/v/unreal_helpers.svg)](https://crates.io/crates/unreal_helpers)

Core crate that provides utilities for wotking with Unreal Engine binary files. It is relied on by all the other binary
parsing crates in this repo.

## License

Licensed under [MIT license](./LICENSE).

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion by you, shall be licensed
as above, without any additional terms or conditions.
