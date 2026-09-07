# NOTICE

Stress Center is a fork of [Mission Center](https://gitlab.com/mission-center-devs/mission-center)
(GPL-3.0-or-later, Copyright Mission Center Developers). This file lists what was changed in this fork, as required
by the GPL when distributing a modified version. The unmodified license text is in [COPYING](COPYING).

Base commit forked from: `mission-center-devs/mission-center` main branch, `fd060f1` ("app: Remove sidebar animation
during switch to Performance page"), 2026-09-07.

## New functionality: the Stress page

A new "Stress" tab was added, driving `stress-ng` from the UI process to load-test CPU and/or memory while charting
live utilization, per-core load, package power and temperature.

New files (no upstream equivalent):

* `src/stress_page/mod.rs` — the `StressPage` widget: controls, graph wiring, log pane, start/stop UI state.
* `src/stress_page/runner.rs` — spawns and supervises `stress-ng`: process-group creation (`setsid`), stdout/stderr
  streaming, and stop (`SIGTERM` to `-pgid`, grace period, then `SIGKILL`).
* `resources/ui/stress_page/page.blp` — the page's Blueprint UI definition.
* `resources/lightning-symbolic.svg` — tab icon for the Stress page.
* `PKGBUILD` — Arch Linux packaging, including `stress-ng` as a runtime dependency.
* `NOTICE.md` — this file.

`stress-ng` runs entirely from the existing, unprivileged UI process via `std::process::Command`. Nothing was added
to, or routed through, the `magpie` gatherer subproject, and the gatherer submodule itself is untouched.

Known limitation: the gatherer (`magpie`) only reports per-core CPU *utilization*, not per-core *clock speed* — there
is no such field in `magpie-types`. Rather than modify the pinned gatherer subproject to add one, the Stress page's
"Per-Core Utilization" graph shows per-core utilization (the closest per-core telemetry actually available) instead
of per-core clocks. Overall clock speed is unaffected and still shown elsewhere in the app as usual.

## Changes to existing upstream files

Kept to the minimum needed to register the new page and rename the fork's identity so it doesn't collide with an
installed copy of Mission Center:

* `src/main.rs` — added `mod stress_page;` and a panic hook that kills any running `stress-ng` process group before
  the default panic behavior runs (stress-ng forks many workers, which would otherwise be orphaned).
* `src/window.rs` — added a `stress_page` template child, `ensure_type()` registration, and forwarded periodic
  readings/animation ticks to it, mirroring how the existing pages already receive them.
* `resources/ui/window.blp` — added one `Adw.ViewStackPage` for the Stress tab; changed the window title and the
  "About" menu label from "Mission Center" to "Stress Center".
* `resources/meson.build` / `resources/missioncenter.gresource.xml` — registered the new `.blp`/`.ui` file and icon.
* `src/application.rs` — added an `ApplicationImpl::shutdown()` override that kills any running `stress-ng` process
  group on app quit; updated the About dialog and GSettings schema ID to the new identity.
* `src/first_run_dialog.rs` — updated one user-visible string ("Restart Mission Center..." → "Restart Stress
  Center...").
* `meson.build` (root) — renamed the project (also renames the installed binary and its data directory); added a
  runtime-only `find_program('stress-ng', ...)` check that warns, but does not fail configuration, if it's missing.
* `src/meson.build`, `po/meson.build` — renamed `APP_ID` and the gettext domain to the new identity.
* `Cargo.toml` — renamed the package to match the new binary name; added a contributor entry.
* `data/io.missioncenter.MissionCenter.{desktop.in,gschema.xml,metainfo.xml.in}` and the two app icons — renamed to
  `io.stresscenter.StressCenter.*` and re-pointed at this fork's identity (app name, icon, GSettings schema ID and
  path, issue tracker URL). `data/meson.build` and `data/icons/meson.build` updated to match.

Deliberately **not** changed, to keep the diff against upstream small and rebases tractable:

* Internal GResource resource paths (still `/io/missioncenter/MissionCenter/ui/...`) and internal Rust type names
  (`MissionCenterWindow`, `MissionCenterApplication`, etc.) — these are invisible to users and upstream's own code
  already treats the resource path as independent from the app ID (see the comment in `src/main.rs` above
  `set_resource_base_path`).
  Internal log-domain strings (e.g. `"MissionCenter::Application"` in `g_critical!`/`g_message!` calls).
* The Performance, Apps and Services pages, the `magpie` gatherer, and the `graph-widget` subproject — all
  unmodified upstream code.
* The Flatpak manifest and Snap packaging — left as-is and not maintained by this fork; only native/PKGBUILD builds
  are supported here.

## Copyright

Files substantially modified for this fork carry an added copyright line alongside the original Mission Center
Developers' notice. Upstream copyright headers are otherwise left intact throughout.
