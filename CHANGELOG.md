# Changelog

## 0.2.1

### Added

- **GPU graph** on the overview: best-effort load from Adreno/Mali sysfs
  (`gpu_busy_percentage`, `gpubusy`, `utilisation`, devfreq), shown as a live
  sparkline with the current percentage.
- **Three more light themes**: Sand (warm beige), Mint (light green) and Rose
  (light pink), alongside the existing Light theme.

### Fixed

- CPU load no longer double-counts `guest`/`guest_nice` ticks from `/proc/stat`
  and is smoothed over samples to avoid jitter.
- Sparkline point counters are localized (RU/EN).

## 0.2.0

### Added

- **Apps tab**: search user packages, open app settings, force stop, clear data,
  disable/enable, uninstall and read `dumpsys package` info.
- **Info tab**: full device dump (`getprop`, `ip addr`, `df`, `uname`, battery,
  display, CPU info) with copy support.
- **Ping**: ADB round-trip and device-to-internet ping (`8.8.8.8`).
- **Screen streaming**: configurable 150–3000 ms refresh, tap control and PNG export.
- **Custom buttons**: add, edit, remove and reset buttons with arbitrary `adb shell`
  commands; presets for Play/Pause, Menu, Camera and volume.
- **Themes**: Graphite, Ocean, Emerald, Violet and Light.
- **Localization**: Russian and English UI, including publish logs.
- **CI and releases**: GitHub Actions test/build on Windows, Linux and macOS,
  best-effort FreeBSD build, multi-OS assets on tag push.

### Fixed

- CPU load is now calculated from `/proc/stat` deltas instead of the inaccurate
  per-process value from `dumpsys cpuinfo`.
- Process rows no longer render with unreadable text.

## 0.1.0

- Initial release: overview, processes, screenshot, console, Wi-Fi connection and
  one-click GitHub publishing.
