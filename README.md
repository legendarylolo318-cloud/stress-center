<img align="left" src="data/icons/hicolor/scalable/apps/io.stresscenter.StressCenter.svg" alt="Stress Center icon" width="64"/>

# Stress Center

**Stress Center is a fork of [Mission Center](https://gitlab.com/mission-center-devs/mission-center)**, the GTK4/libadwaita
system monitor, with one addition: a built-in **Stress** page that drives
[`stress-ng`](https://github.com/ColinIanKing/stress-ng) to load-test your CPU and memory while showing live
utilization, per-core load, package power and temperature graphs. Everything else — the Performance, Apps and
Services pages — is unmodified Mission Center. See [NOTICE.md](NOTICE.md) for the full, itemized list of changes.

This is an independent fork, not affiliated with or endorsed by the Mission Center project.

![](https://gitlab.com/mission-center-devs/mission-center/-/raw/main/screenshots/0001-cpu.png)

## What's different from Mission Center

* A new **Stress** tab alongside Performance/Apps/Services, added without modifying how those existing pages work.
* Controls for test type (CPU / Memory / CPU and Memory), worker count (defaults to your core count), duration,
  a CPU stress method picked from `stress-ng --cpu-method which`, and a `--verify` toggle (on by default).
* stress-ng's stdout/stderr are streamed live into a log pane in the page, with `--verify` failures and other
  errors highlighted in a distinct style so they're easy to spot.
* The existing CPU/temperature/power graph widgets are reused to chart utilization, per-core load, package power
  and temperature while a test runs, with the time range the test was active shaded on the graphs.
* Stop reliably tears down the whole `stress-ng` process group (it forks many workers), including on app quit or
  a crash — not just the direct child process.
* `stress-ng` is invoked directly from this (unprivileged) UI process; nothing was added to, or routed through,
  the `magpie` gatherer process.
* Renamed application ID, binary and desktop file (`io.stresscenter.StressCenter` / `stress-center`) so it can be
  installed alongside a real Mission Center without colliding.

Full details, including exactly which files were touched and why, are in [NOTICE.md](NOTICE.md).

## Everything Mission Center already does

* Monitor overall or per-thread CPU usage
* See system process, thread, and handle count, uptime, clock speed (base and current), cache sizes
* Monitor RAM and Swap usage
* See a breakdown how the memory is being used by the system
* Monitor Disk utilization and transfer rates
* Monitor network utilization and transfer speeds
* See network interface information such as network card name, connection type (Wi-Fi or Ethernet), wireless speeds
  and frequency, hardware address, IP address
* Monitor overall GPU usage, video encoder and decoder usage, memory usage and power consumption, powered by the
  popular NVTOP project
* See a breakdown of resource usage by app and process
* Supports a minified summary view for simple monitoring
* Uses GTK4 and Libadwaita, written in Rust

For the full upstream feature list and background, see the
[Mission Center README](https://gitlab.com/mission-center-devs/mission-center/-/blob/main/README.md).

## Installing

There are no prebuilt packages for this fork yet — build it from source (below), or use the included `PKGBUILD` on
Arch-based distributions:

```bash
makepkg -si
```

If you just want Mission Center itself (not the stress-testing addition), see its
[own install options](https://gitlab.com/mission-center-devs/mission-center#installing).

## Building and running

Requirements are the same as upstream Mission Center, plus `stress-ng` at runtime for the Stress page (it's a
runtime dependency, not a build dependency — `meson setup` will warn, not fail, if it's missing).

**Requirements:**

| Dependency                   | Comment                          | Minimum Version |
|-------------------------------|----------------------------------|----------------:|
| Meson                         |                                   |           1.0.2 |
| Rust                          |                                   |            1.90 |
| CMake                         |                                   |            3.15 |
| Python3                       |                                   |            3.10 |
| Python GObject Introspection  | Used by Blueprint Compiler       |             N/A |
| DRM development libraries     |                                   |             N/A |
| GBM development libraries     |                                   |             N/A |
| udev development libraries    |                                   |             N/A |
| GTK 4                         |                                   |            4.22 |
| libadwaita                    |                                   |             1.9 |
| stress-ng                     | Runtime only, for the Stress page|             N/A |

**Build instructions**

```bash
# Avoid using "--depth=1" as it will not include submodules and the build will fail
git clone https://github.com/legendarylolo318-cloud/stress-center --recursive
cd stress-center

# On Ubuntu 26.04 all dependencies, except for the Rust toolchain and stress-ng, can be installed with:
sudo apt install build-essential cmake curl desktop-file-utils gettext git libadwaita-1-dev libdbus-1-dev libdrm-dev libgbm-dev libudev-dev meson pkg-config protobuf-compiler python3-gi python3-pip stress-ng

BUILD_ROOT="$(pwd)/build-meson-debug"

meson setup "$BUILD_ROOT" -Dbuildtype=debug # Alternatively pass `-Dbuildtype=release` for a release build
ninja -C "$BUILD_ROOT"
```

If you want to run the application from the build directory (for development or debugging) some set up is required:

```bash
export PATH="$BUILD_ROOT/subprojects/magpie/src:$PATH"
export GSETTINGS_SCHEMA_DIR="$BUILD_ROOT/data"
export MC_MAGPIE_HW_DB="$BUILD_ROOT/subprojects/magpie/platform-linux/hwdb/hw.db"
export MC_RESOURCE_DIR="$BUILD_ROOT/resources"

glib-compile-schemas --strict "$(pwd)/data" && mv "$(pwd)/data/gschemas.compiled" "$BUILD_ROOT/data/"
```

And then to run the app:

```bash
"$BUILD_ROOT/src/stress-center"
```

If you want to install the app just run:

```bash
ninja -C "$BUILD_ROOT" install
```

And run the app from your launcher or from the command-line:

```bash
stress-center
```

Flatpak, Snap and AppImage packaging from upstream are present in this fork's tree but are **not maintained here**
and are not expected to build correctly — this fork only targets native/PKGBUILD builds.

## Rebasing on upstream

This fork is intentionally structured to keep the diff against upstream Mission Center small: the Stress page lives
entirely in new files (`src/stress_page/`, `resources/ui/stress_page/`), and existing files are touched only at the
minimal points needed to register the new page and rename the app's identity. See NOTICE.md for the exact list.

## Contributing / Issues

Issues and contributions for the Stress page addition are welcome via this repository's
[issue tracker](https://github.com/legendarylolo318-cloud/stress-center/issues). For anything about Mission Center
itself (unrelated to the Stress page), please use the
[upstream issue tracker](https://gitlab.com/mission-center-devs/mission-center/-/issues) instead.

### Translations

Translations are inherited from upstream Mission Center's `.po` files and have not been updated for the new Stress
page strings. Contributions to translate the new strings are welcome.

## License

This program is free software; you can redistribute it and/or modify it under the terms of the GNU General Public
License as published by the Free Software Foundation; either version 3 of the License, or (at your option) any later
version.

Please see the [COPYING](COPYING) file in the root of this repository for the complete license text (unchanged from
upstream). Alternatively see [the official license](https://www.gnu.org/licenses/gpl-3.0.html) as written by the
Free Software Foundation.

## Code of Conduct

This fork follows the GNOME Code of Conduct, same as upstream Mission Center. All communications in project spaces
are expected to follow it.
