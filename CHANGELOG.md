# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html) since v0.2.0.

## [Unreleased]
### Added
- [audio] Add support for dithering with `--dither` for lower requantization error (breaking)
- [audio] Add support for noise shaping with `--shape-noise` for lower perceived noise (breaking)
- [playback] Add `--volume-range` option to set dB range and control `log` and `cubic` volume control curves
- [playback] Support for querying dB ranges from `alsamixer` Softvol

### Changed
- [connect, playback] Moved volume controls from `connect` to `playback` crate
- [playback] Normalize volumes to [0.0..1.0] instead of [0..65535] for greater precision and performance (breaking)
- [playback] Complete rewrite of `alsamixer` (breaking)
- [playback] Make cubic volume control available to all mixers with `--volume-ctrl cubic`
- [playback] Use dB range from `alsamixer` for the `log` volume control unless specified otherwise
- [playback] Use `--device` name for `--mixer-card` unless specified otherwise

### Fixed
- [connect] Fix step size on volume up/down events
- [playback] Make `--volume-ctrl {linear|log}` work as expected on `alsamixer`
- [playback] Fix `log` and `cubic` volume controls to be mute at zero volume
- [playback] Make `cubic` consistent between cards that report minimum volume as mute, and cards that report some dB value

### Removed
- [connect] Removed no-op mixer started/stopped logic
- [playback] Removed `--mixer-linear-volume` option; use `--volume-ctrl linear` instead

## [0.2.0] - 2021-05-04

## [0.1.6] - 2021-02-22

## [0.1.5] - 2021-02-21

## [0.1.3] - 2020-07-29

## [0.1.2] - 2020-07-22

## [0.1.1] - 2020-01-30

## [0.1.0] - 2019-11-06

[unreleased]: https://github.com/librespot-org/librespot/compare/v0.2.0..HEAD
[0.2.0]: https://github.com/librespot-org/librespot/compare/v0.1.6..v0.2.0
[0.1.6]: https://github.com/librespot-org/librespot/compare/v0.1.5..v0.1.6
[0.1.5]: https://github.com/librespot-org/librespot/compare/v0.1.3..v0.1.5
[0.1.3]: https://github.com/librespot-org/librespot/compare/v0.1.2..v0.1.3
[0.1.2]: https://github.com/librespot-org/librespot/compare/v0.1.1..v0.1.2
[0.1.1]: https://github.com/librespot-org/librespot/compare/v0.1.0..v0.1.1
[0.1.0]: https://github.com/librespot-org/librespot/releases/tag/v0.1.0
