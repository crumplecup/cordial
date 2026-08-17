# cordial

[![License](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue.svg)](LICENSE-APACHE)

**Polite standards for code development.**

`cordial` is a plugin framework for local, regeneratable reports about whether a
codebase follows the etiquettes you care about. It refines
[`elicit_doc`](https://github.com/crumplecup/elicit_doc) with a trait-based
architecture: loaders, enrichers, probes, assessors, and reporters hook into a
shared graph IR so users can register custom lints without forking the tool.

Artifacts land under `~/.cordial/{project}/` and are never committed to git.

## Status

Early design. See [CORDIAL_PLAN.md](CORDIAL_PLAN.md) for architecture and
implementation phases.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
