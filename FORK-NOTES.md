# Fork notes

This repository is a **modified copy** of PecoFence, not the upstream project.

---

## Upstream

| | |
|---|---|
| **Project** | PecoFence — a free, open-source Stardock Fences alternative for Windows |
| **Author** | DayuanJiang and contributors |
| **Repository** | https://github.com/DayuanJiang/PecoFence |
| **Website** | https://pecofence.jiang.jp/ |
| **Licence** | Apache License 2.0 (see [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE)) |
| **Base snapshot** | The `main` branch as of late October 2026; `Cargo.toml` reports `0.2.3` |

> **This fork is not endorsed by, affiliated with, or supported by the upstream author.**
> For official builds, downloads, documentation and support, use the upstream repository.
>
> Upstream has since released **0.2.4** and **0.2.5**. Their fixes are **not** included here. If you
> want the maintained version, take upstream.

---

## What this fork changes

The work here is deliberately narrow: **start-up behaviour, idle power and menu behaviour on a busy
Windows desktop**. It is not a feature fork, and it does not try to replace upstream.

Every file this fork touched is listed below, as Apache-2.0 §4(b) requires.

| File | Change |
|---|---|
| `crates/platform/src/frameclock.rs` | **Idle wake-up self-heal.** Adds `ARMED_FORCE_DISARM_MS = 500` and an `armed_since` stamp. If the compositor clock stays armed for 500 ms without a fresh `request()`, the worker force-disarms it and logs a warning. Without this, a caller that forgets to release the clock after an animation finishes leaves the compositor waking an idle app forever — measurable battery drain with the window untouched. |
| `crates/app/src/app/sync.rs` | **Boot-silent event filter.** Adds `is_boot_silent_noise()` plus `BOOT_SILENT_NOISE_EXTS` (`tmp`, `crdownload`, `part`, `partial`, `download`, `!ut`, `!qb`, `!bc`, `dat`) and `BOOT_SILENT_NOISE_PREFIXES` (`~`, `.~`). During the first 30 s after start-up, transient files that browsers, installers and updaters drop on the desktop are dropped from the watcher batch, so a logon storm cannot drive continuous relayout. `Overflow` events are never filtered, so a full re-enumeration still happens. Names without an extension are deliberately **not** dropped wholesale — a folder dropped on the desktop in those 30 s must still appear. |
| `crates/app/src/app/mod.rs` | **Start-up coalescing windows.** Widens the watcher debounce during boot (`FS_BOOT_DEBOUNCE_MS = 1_000`, `FS_BOOT_STORM_MS = 2_000`), bounds boot-silent mode to `BOOT_SILENT_SECS = 30`, and delays hiding the Windows desktop icons by `HIDE_ICONS_DELAY_MS = 2_000` so the hide lands after Explorer's own logon storm instead of competing with it. Adds two timers: `TIMER_HIDE_ICONS_DELAYED` (67) and `TIMER_BOOT_SILENT_END` (68). |
| `crates/app/src/app/menus.rs` | **Native-only shell menus.** Replaces the self-drawn menu entries that were prepended above shell entries (open, rename, move-to-fence and friends) with the pure native Shell menu, and gives every menu command an explicit `CMD_*` id with documented ranges (`CMD_SHOW_FOLDER` 11..=14, fence-menu folder entries 390..=393, merge entries offset from `host_fences()`). Fewer custom entries means fewer menu rebuilds and a context menu that matches Explorer's own, extensions included. |
| `crates/app/build.rs` | **Non-ASCII path fix for `rc.exe`.** `rc.exe` is an ANSI tool: it cannot open a path its code page cannot represent, so a checkout under a Chinese (or any non-ASCII) directory failed to compile and shipped an executable with no icon and no version resource. The build script now copies the icon next to itself, references it relatively and runs the compiler with the output directory as its working directory, so every path the tool sees is ASCII. |
| `crates/app/src/app/visuals.rs` | **Menu theming follows Windows, not the fence theme.** Windows colours an application's menus from its *app* colour (`AppsUseLightTheme`), not from the mode the fences follow, so a menu forced dark by the fence theme looked unlike Explorer's own desktop menu. Menus now follow the app colour; only an explicitly chosen light/dark theme (or `--light` / `--dark`) colours them along with the fences. Also bounds how long `StartupBackdrops::finish` waits for decode and blur before falling back to the solid backdrop, so a slow wallpaper cannot hold up start-up. |

### Files added by this fork

| File | Purpose |
|---|---|
| `docs/PRD.md` | Requirements document for this fork's start-up performance work |
| `docs/TECHNICAL-DESIGN.md` | Technical design behind the changes above |

### File removed by this fork

| File | Reason |
|---|---|
| `crates/platform/src/colordialog.rs` | Folded into the native menu path above; the custom colour dialog is no longer reachable once menus are native-only |

### Not built by this fork

Two things in the tree are **upstream work, not this fork's**, and are noted here so nothing is
claimed twice:

- **Custom themes** (`crates/render/src/custom_theme.rs`, `ui/themes/*.json`, `docs/THEMES.md`) —
  upstream, first committed 2026-10-18, extended 2026-11-01.
- **Italian localisation** (`locales/it.json` and the `*it*` files under `docs/` and `site/`) —
  upstream, committed 2026-10-31.

A handful of other files carry slightly different byte counts than the base snapshot
(`crates/app/src/state.rs`, `icons.rs`, `crates/platform/src/shell.rs`, `tray.rs`,
`crates/core/src/rules.rs`, `rust-toolchain.toml`). Their timestamps moved but the content
difference is upstream drift between the base snapshot and this copy, not a fork edit — the fork
made no intentional change there. If in doubt, diff against upstream.

---

## Building

Identical to upstream. See [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md).

```powershell
cargo build --release
cargo test
```

A full build needs the MSVC toolchain, the Windows SDK and WebView2; upstream's CI files under
`.github/workflows/` document the exact setup.

---

## Licence compliance

Apache-2.0 obligations, and where each is met:

| Obligation | Where |
|---|---|
| Ship the licence text | [`LICENSE`](LICENSE), unchanged |
| Keep the upstream `NOTICE` | [`NOTICE`](NOTICE) — upstream text preserved, fork attribution appended below it |
| State which files were changed | This document, above |
| No implied endorsement | Stated at the top of this file and in [`README.md`](README.md) |
| Preserve third-party notices | [`third_party/README.md`](third_party/README.md), unchanged |

If you redistribute a build of this fork, keep those files with it.

---

## Upstream contributions

If any change here is useful upstream, it belongs in a pull request against
[DayuanJiang/PecoFence](https://github.com/DayuanJiang/PecoFence) rather than only in this copy.
Open an issue there first — the maintainer has opinions about menu behaviour, and rightly so.
