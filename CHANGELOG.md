# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Added
- `await_py` functions to `any`, `tokio`, `PyLoop` and `ThreadLocals` for calling a
  python function in the event loop's thread then awaiting its result.
- `run` functions for starting the Python event loop with a Rust future.

### Changed
- `tokio::fut_into_coro_with_locals`, `tokio::local_fut_into_coro_with_locals`,
  `any::local_fut_into_coro` and `any::fut_into_coro` now return `PyResult<&PyAny>`.

### Fixed
- `fut_into_coro` methods no-longer panic when called in a runtime where Anyio isn't
  installed.
- `fut_into_coro` now delay initialising the internal `Event` to avoid runtime errors
  being raised if the event loop hasn't been started yet.
- `PyLoop` `call_{}` and `coro_to_fut` methods + the relevant top level functions
  now consistently error if the Python event loop isn't running regardless of the loop.

[Unreleased]: https://github.com/FasterSpeeding/Alluka/compare/v0.1.0...HEAD
