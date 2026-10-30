# Implementation Plan: Linux version, release 1

Spec: [`SPEC.md`](../SPEC.md). Root branch: `feature/linux-r1`. Task list: [`todo.md`](todo.md).

## Overview

Release 1 brings the core of the app to GNOME and KDE on Wayland: Alt+M, transcription, the text typed at the cursor, undo, the tray icon, start at login, and an install from GitHub Releases and the AUR. The work happens on `feature/linux-r1`. It reaches `main` only after the regression tests pass (SPEC.md, "Branches and tracking").

Release 2 (meter, GTK windows, live paste, dark mode) gets its own plan when release 1 is merged. Only the meter test is done early, because its result can change how much work release 2 is.

## Architecture decisions

- **Shared code in `core/`, a library crate.** It holds audio, deepgram, words, gfx, art, session and live, plus the portable half of config (`.env` parsing). `rust/` (Windows) and `linux/` depend on it with `path = "../core"`. There is no workspace, so `rust/target/release/dictation.exe` stays where it is.
- **No platform types in `core/`.** `session.rs` and `live.rs` use `HWND` today. In `core/` this becomes `pub type WindowId = isize`. Windows converts `HWND` to and from it. On GNOME, Linux uses `0` ("unknown"), because Wayland does not say which window has focus.
- **`core/` does not choose the paths.** `words.rs`, `logging.rs` and `config.rs` look next to the `.exe` today. In `core/`, the app passes the folder in at start. Windows passes the `.exe` folder, and Linux passes the XDG folders (`~/.config/dictation`, `~/.local/state/dictation`).
- **Logging in `core/` writes through a hook.** `core::log` formats the line and hands it to a writer that the app installs at start. Windows keeps `GetLocalTime`; Linux uses `libc::localtime_r`.
- **The `overlay::State` enum** (idle, recording, transcribing, …) moves to `core::session`, because `session.rs` needs it and it has no Windows code.
- **Tests first for risky platform code.** Portal behavior cannot run in CI, so each portal gets a small test program (spike) on GNOME 51 before the real code is written. The spike results are recorded in `docs/linux-spikes.md` and answer the spec's open questions.
- **Windows is checked from Linux** with `cargo check --target x86_64-pc-windows-gnu` (it compiles, without linking). Windows tests run in CI on every push to `feature/linux-r1`.

## Dependency graph

```text
T1 Rust toolchain on this machine
 ├── S1 spike: typing (RemoteDesktop + libei)   ─┐
 ├── S2 spike: clipboard                         ├─ results → docs/linux-spikes.md
 ├── S3 spike: hotkeys (GlobalShortcuts)         │
 ├── S4 spike: meter on top (XWayland)          ─┘  (decides release 2 work)
 │
 ├── T2 CI: run on the root branch, add Linux job
 │    └── T3 core/: gfx + art (and build.rs)
 │         └── T4 core/: logging hook, paths in, audio + words + deepgram + .env parsing
 │              └── T5 core/: session + live (WindowId, State)
 │                   └── T6 linux/: skeleton: record → transcribe → log, --toggle
 │                        ├── T7 tray + notifications
 │                        ├── T8 type at cursor          (needs S1)
 │                        │    ├── T9 clipboard copy     (needs S2)
 │                        │    └── T11 undo              (needs T10)
 │                        ├── T10 hotkeys via portal     (needs S3)
 │                        └── T12 desktop file, icons, autostart, first-run key
 │                             └── T13 release tarball → T14 AUR → T15 docs
 └── T16 regression tests → release PR into main
```

## Task list

### Phase 0: Setup and spikes (the risky parts first)

The spikes are throwaway programs in `spikes/<name>/`, each its own small crate. They answer one question each. Keep the code only if a later task can reuse it. S1–S4 depend only on T1 and do not touch shared files, so they can run in parallel with each other and with T2–T5.

- T1: Install the Rust toolchain on this machine
- W1: Set up a Windows 11 VM for the Windows checks
- S1: Spike: type text through the RemoteDesktop portal and libei
- S2: Spike: set the clipboard from the background on GNOME
- S3: Spike: register Alt+M through the GlobalShortcuts portal
- S4: Spike: does an XWayland override-redirect window stay on top on GNOME

### Checkpoint A: spikes done

- [ ] `docs/linux-spikes.md` answers spec open questions 1–4, each with a yes, no, or "fallback used".
- [ ] SPEC.md is updated where a fallback is now the plan.
- [ ] Review with the maintainer. If S1 fails (no typing on GNOME), stop and re-plan.

### Phase 1: Shared code into `core/` (Windows must stay the same)

Each task leaves the Windows app building and passing its tests. These tasks change `rust/Cargo.toml` and `rust/src/main.rs`, so they run one after another.

- T2: CI runs on `feature/linux-r1`, with a Linux job
- T3: Create `core/` with gfx and art
- T4: Move audio, words, deepgram, logging and `.env` parsing into `core/`
- T5: Move session and live into `core/`

### Checkpoint B: core done

- [ ] CI is green on Windows and Linux for `core/` and `rust/`.
- [ ] The test count in `rust/` + `core/` equals the test count in `rust/` before T3 (no test lost).
- [ ] Maintainer runs the Windows app from a CI build: Alt+M, live paste, undo, tray, Settings, Custom words all work.

### Phase 2: Linux app, end to end

- T6: Linux skeleton: `dictation --toggle` records, transcribes, writes the text to the log
- T7: Tray icon and menu, and notifications
- T8: Type the transcript at the cursor
- T9: One clipboard copy per dictation
- T10: Alt+M, Ctrl+Alt+Z, Ctrl+Alt+Q through the GlobalShortcuts portal
- T11: Undo with Ctrl+Alt+Z

### Checkpoint C: Linux works on this machine

- [ ] Alt+M → speak → Alt+M types the text into gedit, Firefox and GNOME Terminal.
- [ ] Ctrl+Alt+Z deletes it. Ctrl+Alt+Q quits. The tray shows every state.
- [ ] Memory of the running app below 30 MB.
- [ ] Review with the maintainer.

### Phase 3: Install and docs

- T12: Desktop file, icons, start at login, first start without a key
- T13: Release workflow builds the Linux tarball
- T14: AUR `-bin` package
- T15: `docs/linux.md` and a Linux section in the README

### Checkpoint D: release 1 regression (gate for `main`)

- T16: Full regression run, then the release pull request from the fork into `rawasmhd:main`

## Parallel work

| Can run at the same time | Must run in order |
|---|---|
| S1, S2, S3, S4, and T2–T5 | T3 → T4 → T5 (all edit `rust/Cargo.toml` and `rust/src/main.rs`) |
| T7 and T8 (after T6), if T7 owns `tray.rs` and T8 owns `typing.rs`, and only one of them edits `linux/src/main.rs` at a time | T6 before every other `linux/` task |
| T14 and T15 | T13 before T14 (the AUR package needs the tarball) |

`README.md`, `CLAUDE.md`, `rust/Cargo.toml` and `linux/Cargo.toml` are shared files: one task at a time.

## Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| The RemoteDesktop portal or libei does not type on GNOME 51 | High: no text at the cursor | S1 first. Fallback: clipboard + Ctrl+V through the portal; last resort ydotool (needs approval, SPEC "Ask first") |
| `reis` (libei crate) is young and changes its API | Medium | Pin the exact version; keep libei code in `typing.rs` only |
| Moving code to `core/` breaks the Windows app in a way the tests do not catch | High | One module group per task; `cargo check --target x86_64-pc-windows-gnu` locally; CI Windows tests; manual Windows check at Checkpoint B |
| `main` changes while the branch is open and conflicts with the moved files | Medium | Merge `upstream/main` into the root branch after every change to `rawasmhd`'s `main` (SPEC "Branches and tracking", item 3) |
| GNOME forgets the typing permission after a reboot | Low | S1 tests it; worst case one dialog per login, documented |
| The app ID is missing when started from a terminal, so portals misbehave | Medium | T6 registers with `org.freedesktop.host.portal.Registry` first; start with `gtk-launch` |
| The Windows 11 VM behaves differently from a real PC (microphone, hotkeys, start at login) | Medium: a Windows bug could slip through | W1 checks the microphone and hotkeys first; anything the VM cannot test is listed in the release pull request |

## Decided

- The Windows checks (Checkpoints B and D) run in a Windows 11 VM, set up in W1. There is no Windows PC.

## Open questions

- Development and the Linux tests run on the maintainer's own GNOME 51 machine, not in a VM: it is the release 1 target, and the app needs no root. VMs are for the Windows checks and for install tests on other distributions (Ubuntu, Fedora, KDE) before release.
- Should the spikes stay in the repository (`spikes/`) after release 1, or be deleted once `docs/linux-spikes.md` has their results?
