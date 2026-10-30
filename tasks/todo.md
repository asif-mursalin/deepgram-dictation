# Tasks: Linux version, release 1

Root branch: `feature/linux-r1`. Plan: [`plan.md`](plan.md). Spec: [`SPEC.md`](../SPEC.md).

The work happens in the fork `asif-mursalin/deepgram-dictation` (`origin`). `upstream` is `rawasmhd/deepgram-dictation`, fetched only, never pushed to. Each task branch starts from `origin/feature/linux-r1`, is named `<type>/<task>-<short-name>` (for example `feature/T8-typing`), and merges back with a pull request inside the fork. The GitHub issues are posted before the release pull request into `main`.

**Every task is done only when** (unless the task says otherwise): `cargo build` has no warnings and `cargo test` passes in each crate it changed, the Windows app still compiles (`cd rust && cargo check --target x86_64-pc-windows-gnu`), and it changed only the files listed.

Sizes: XS = 1 file, S = 1–2 files, M = 3–5 files.

---

## Phase 0: Setup and spikes

- [x] **T1: Install the Rust toolchain on this machine** (XS, no repo files)
  - Do: `sudo pacman -S rustup && rustup default stable && rustup target add x86_64-pc-windows-gnu`. The build packages `alsa-lib`, `libxkbcommon`, `pkgconf` and `pipewire-alsa` are already installed.
  - Done when: `cargo --version` works, and `cd rust && cargo check --target x86_64-pc-windows-gnu` finishes without errors.
  - Depends on: none

- [ ] **W1: Set up a Windows 11 VM for the Windows checks** (no repo files)
  - Do:
    - Create a Windows 11 VM in virt-manager (with TPM and Secure Boot, which Windows 11 needs), using the SPICE display and sound so the microphone is passed through.
    - Turn off Smart App Control in the VM, because the `.exe` is not signed (#19).
    - Copy a `.env` with the Deepgram key next to the `.exe`.
    - Start the app with `explorer.exe`, as in CLAUDE.md.
  - Done when: the current release `dictation.exe` (from `main`, before any core change) works in the VM. Alt+M, speak, Alt+M types the text into Notepad, and the meter and tray show. That run is the baseline for Checkpoints B and D. Write down anything the VM cannot test.
  - Depends on: none (needed before Checkpoint B)

- [x] **S1: Spike: type text through the RemoteDesktop portal and libei** (S, `spikes/typing/`, `docs/linux-spikes.md`)
  - Do: a small program that opens a keyboard-only RemoteDesktop session (`ashpd`), connects to EIS (`reis`), and types "Hello, world." into the focused window after 3 seconds. Save the restore token and reuse it on the next run. Print the keymap layout that libei reports while English (US) is active, then while Arabic is active.
  - Done when: `docs/linux-spikes.md` answers these questions:
    - Does typing work in gedit, Firefox and GNOME Terminal?
    - Is the dialog skipped on the second run?
    - Is it skipped after a reboot? (spec open question 2)
    - Does the app learn the active layout? (spec open question 4)
  - Depends on: T1

- [x] **S2: Spike: set the clipboard from the background on GNOME** (S, `spikes/clipboard/`, `docs/linux-spikes.md`)
  - Do: try the portal Clipboard interface (tied to a RemoteDesktop session), then `arboard` through XWayland. With another window focused, set the text, then press Ctrl+V in gedit.
  - Done when: `docs/linux-spikes.md` names the approach that works, or says neither does (spec open question 1). It also says whether the entry can be marked "do not keep in history".
  - Depends on: T1

- [ ] **S3: Spike: register Alt+M through the GlobalShortcuts portal** (S, `spikes/hotkeys/`, `docs/linux-spikes.md`)
  - Do: bind three shortcuts with preferred triggers Alt+M, Ctrl+Alt+Z and Ctrl+Alt+Q, and print each activation. Run it twice.
  - Done when: `docs/linux-spikes.md` records:
    - what GNOME 51 shows the first time;
    - whether the second run asks again;
    - whether Alt+M is accepted or GNOME proposes another key.
  - Depends on: T1

- [ ] **S4: Spike: does an XWayland override-redirect window stay on top on GNOME** (S, `spikes/meter/`, `docs/linux-spikes.md`)
  - Do: with `x11rb`, open a 320×48 ARGB override-redirect window at the bottom center, with an empty input shape. Click and type in a native Wayland app (gedit) under it, and in Firefox.
  - Done when: `docs/linux-spikes.md` records:
    - whether the window stays on top;
    - whether it takes focus;
    - whether clicks pass through.

    That decides between the meter and the GNOME Shell extension for release 2 (spec open question 3).
  - Depends on: T1

### Checkpoint A: spikes done
- [ ] `docs/linux-spikes.md` answers the spec's open questions 1–4
- [ ] SPEC.md is updated where a fallback became the plan
- [ ] Maintainer review. If S1 failed, stop and re-plan

---

## Phase 1: Shared code into `core/`

- [ ] **T2: CI runs on `feature/linux-r1`, with a Linux job** (S, `.github/workflows/ci.yml`)
  - Do: add `feature/linux-r1` to the `push` branches. Add an `ubuntu-24.04` job that installs `pkg-config libasound2-dev libxkbcommon-dev` and runs `cargo build --locked` and `cargo test --locked` in each crate folder that exists (`core/`, `linux/`). Leave the Windows job as it is, but also run `core/` tests there once `core/` exists.
  - Before: turn on GitHub Actions in the fork's **Actions** tab.
  - Done when: a push to `feature/linux-r1` in the fork runs CI, and the Windows job is green.
  - Depends on: none

- [ ] **T3: Create `core/` with gfx and art** (M, `core/Cargo.toml`, `core/src/{lib,gfx,art}.rs`, `rust/Cargo.toml`, `rust/build.rs`, `rust/src/main.rs`)
  - Do: a library crate `dictation-core`. Move `gfx.rs` and `art.rs` with their tests. `rust/` uses `dictation_core::{gfx, art}`. `build.rs` points its `#[path]` at `../core/src/`.
  - Done when:
    - The `gfx` and `art` tests pass in `core/` on Linux.
    - The Windows CI job is green.
    - `dictation.exe` still has its icon (CI artifact, checked in Explorer).
  - Depends on: T1, T2

- [ ] **T4: Move audio, words, deepgram, logging and `.env` parsing into `core/`** (M, `core/src/{audio,words,deepgram,logging,env}.rs`, `rust/src/{main,config,logging}.rs`)
  - Do:
    - `core::logging` has `log()`, plus `set_writer()` for the app's writer and timestamp.
    - `words::path()` and `.env` lookup take a folder from the app.
    - `config.rs` in `rust/` keeps only the registry autostart and calls `core::env` for the key.
    - Windows behavior is unchanged: files still sit next to the `.exe`.
  - Done when:
    - All moved tests pass in `core/` on Linux.
    - Windows CI is green.
    - The Windows CI build still finds `.env` and `words.txt` next to the `.exe` (maintainer check).
  - Depends on: T3

- [ ] **T5: Move session and live into `core/`** (M, `core/src/{session,live}.rs`, `rust/src/{main,overlay}.rs`)
  - Do:
    - `pub type WindowId = isize` replaces `HWND` in `session.rs` and `live.rs`.
    - `overlay::State` moves to `core::session::State`, and `rust/src/overlay.rs` uses it from there.
    - `rust/src/main.rs` converts `HWND` ↔ `WindowId` in its `Host` impl.
  - Done when:
    - All session and live tests (with the fakes) pass in `core/` on Linux.
    - Windows CI is green.
    - `core/` has no `windows-sys` dependency.
  - Depends on: T4

### Checkpoint B: core done
- [ ] CI green on Windows and Linux
- [ ] Test count in `core/` + `rust/` equals the test count in `rust/` before T3
- [ ] Maintainer runs the CI-built `dictation.exe` in the Windows 11 VM (W1): Alt+M, live paste, Paused, undo, tray, Settings, Custom words

---

## Phase 2: Linux app, end to end

- [ ] **T6: Linux skeleton: `dictation --toggle` records, transcribes, writes the text to the log** (M, `linux/Cargo.toml`, `linux/src/{main,paths,control}.rs`)
  - Do:
    - A binary crate `dictation` that depends on `core/` and uses rustls for `tungstenite` and `ureq`.
    - XDG paths: config in `~/.config/dictation/`, log in `~/.local/state/dictation/`.
    - A lock file in `$XDG_RUNTIME_DIR`, and a Unix socket for `--toggle` and `--quit`.
    - An `impl Host`, where `paste` only writes the text to the log for now.
    - Before any portal call, register the app ID with `org.freedesktop.host.portal.Registry`.
  - Done when:
    - `dictation &` and then `dictation --toggle`, speak, `dictation --toggle` → the transcript is in `~/.local/state/dictation/dictation.log`.
    - A second `dictation` exits with "already running".
    - Unit tests for paths and the socket messages pass.
  - Depends on: T5

- [ ] **T7: Tray icon and menu, and notifications** (M, `linux/src/{tray,notify,main}.rs`, `linux/Cargo.toml`)
  - Do: `ksni` tray with the state dot from `core::art`. Menu items: Start/Stop dictation, Settings (opens `.env` in the default editor), Custom words (opens `words.txt`), Start at login (disabled until T12), Open log file, Quit. Errors go to `notify-rust` notifications.
  - Done when: on GNOME with the AppIndicator extension, the icon changes through idle, recording, transcribing and problem, and every menu item works. A Deepgram error with no network shows a notification.
  - Depends on: T6

- [ ] **T8: Type the transcript at the cursor** (M, `linux/src/{typing,main}.rs`, `linux/Cargo.toml`)
  - Do: one RemoteDesktop portal session for the whole run, with the keyboard and the clipboard (S1, S2):
    - Save the restore token in `~/.config/dictation/portal-token`, with mode 0600, after every start.
    - Type with `NotifyKeyboardKeysym` (S2). A new line is the Return keysym.
    - Decide before typing: if every character is printable ASCII, a space or a new line, type it. Otherwise hand the text to T9's paste. Until T9 lands, log it and type nothing.
    - Keep the session open; reopen it when a call fails.
    - Measure a 500-character text. If it is too slow, note it for T9 (paste long texts).
  - Done when:
    - Alt-free test: `dictation --toggle` twice types the text into gedit, Firefox and GNOME Terminal, with English and with Arabic active.
    - The unit tests for the "type or paste" rule and the keysym table pass.
  - Depends on: T6, S1, S2

- [ ] **T9: One clipboard copy per dictation, and the paste fallback** (S, `linux/src/{clipboard,main}.rs`)
  - Do: the Clipboard portal on T8's session (S2):
    - Put the full dictation on the clipboard at the end, once.
    - Answer each `SelectionTransfer` while the app owns the clipboard.
    - When T8 says "paste", set the clipboard and press Ctrl+V (evdev keycodes 29 and 47). That is the same single entry.
    - Offer the `x-kde-passwordManagerHint` type too, for history tools that honor it.
  - Done when:
    - After a dictation, Ctrl+V in another app pastes the full text, and the clipboard history has one entry for it.
    - A dictation with "café" pastes correctly with English and with Arabic active.
  - Depends on: T8, S2

- [ ] **T10: Alt+M, Ctrl+Alt+Z, Ctrl+Alt+Q through the GlobalShortcuts portal** (S, `linux/src/{hotkey,main}.rs`)
  - Do: bind the three shortcuts with preferred triggers, and reuse the session so the dialog appears only once. If the portal is missing, log it, and keep `--toggle` working.
  - Done when: after the first dialog, Alt+M starts and stops a dictation from any app, Ctrl+Alt+Q quits, and a restart does not show the dialog again.
  - Depends on: T6, S3

- [ ] **T11: Undo with Ctrl+Alt+Z** (S, `linux/src/{typing,main}.rs`)
  - Do: implement `Host::backspaces` with libei. `typed_since_reset` always returns false on GNOME (spec decision).
  - Done when: Ctrl+Alt+Z right after a dictation deletes exactly that text in gedit and Firefox.
  - Depends on: T8, T10

### Checkpoint C: Linux works on this machine
- [ ] Alt+M → speak → Alt+M types into gedit, Firefox, GNOME Terminal
- [ ] Ctrl+Alt+Z deletes it. Ctrl+Alt+Q quits. The tray shows every state
- [ ] `ps -o rss= -C dictation` below 30000 (KB)
- [ ] Maintainer review

---

## Phase 3: Install and docs

- [ ] **T12: Desktop file, icons, start at login, first start without a key** (M, `linux/src/{autostart,install,main}.rs`, `linux/data/io.github.rawasmhd.DeepgramDictation.desktop`)
  - Do:
    - `dictation --install-desktop` writes the `.desktop` file and the hicolor PNG icons (drawn by `core::art`) to `~/.local/share/`.
    - Start at login writes or removes `~/.config/autostart/io.github.rawasmhd.DeepgramDictation.desktop`.
    - Without a key: ask in the terminal if there is one, and check the key with Deepgram. Otherwise show a notification with the `.env` path.
  - Done when:
    - The app starts after a reboot when start at login is on.
    - `gtk-launch io.github.rawasmhd.DeepgramDictation` starts it.
    - A first start with no key leads to a saved key (mode 0600).
    - The unit test for the autostart file passes.
  - Depends on: T7

- [ ] **T13: Release workflow builds the Linux tarball** (S, `.github/workflows/release.yml`, `linux/packaging/install.sh`)
  - Do:
    - Add an `ubuntu-24.04` job that builds `linux/` in release mode and packs `dictation`, the `.desktop` file, the icons and `install.sh` into `dictation-linux-x86_64.tar.gz`.
    - Attach it to the same GitHub release as `dictation.exe`.
    - Release tags come only from `main`, so test it with `workflow_dispatch` on the branch.
  - Done when: a manual run of the workflow on the branch produces the tarball. Unpacking it and running `./install.sh` on this machine installs a working app.
  - Depends on: T12

- [ ] **T14: AUR `-bin` package** (S, `linux/packaging/aur/PKGBUILD`, `linux/packaging/aur/.SRCINFO`)
  - Do:
    - A `deepgram-dictation-bin` PKGBUILD that downloads the release tarball and installs it to `/usr/bin` and `/usr/share`.
    - Depends on `alsa-lib` and `libxkbcommon`.
    - Publishing to the AUR is a separate step, done by the maintainer.
  - Done when: `makepkg -si` with a test tarball installs a working app, and `namcap PKGBUILD` shows no errors.
  - Depends on: T13

- [ ] **T15: `docs/linux.md` and a Linux section in the README** (S, `docs/linux.md`, `README.md`)
  - Do: install (tarball, AUR), the two permission dialogs, the tray extension, the differences from Windows (undo cannot cancel itself, no Paused state, layout fallback), troubleshooting, and the manual checklist from SPEC.md. The README gets a short Linux section that links `docs/linux.md`. `README.md` is a shared file, so only this task edits it.
  - Done when: a reader can install and use the app from `docs/linux.md` alone.
  - Depends on: T12 (T14 for the AUR part)

---

## Checkpoint D: release 1 regression, gate for `main`

- [ ] **T16: Full regression run, then the release pull request into `main`** (no new code)
  - Do:
    1. `git fetch upstream` and merge `upstream/main` into `feature/linux-r1`. CI in the fork must be green.
    2. **Windows:** in the Windows 11 VM (W1), run the CI-built `dictation.exe` and check:
       - Alt+M and live paste
       - Paused
       - Ctrl+Alt+Z
       - the meter and the tray menu
       - Settings and Custom words
       - start at login
       - Ctrl+Alt+Q

       Then run `bench/` and compare it with the last result in `bench/results/`.
    3. **Linux:** run the manual checklist in SPEC.md (Testing Strategy) and the release 1 success criteria.
    4. Post the tracking issue and one issue per task. Open the pull request `asif-mursalin:feature/linux-r1` → `rawasmhd:main` that links them.
  - Done when: every check passes, with the results written in the pull request, and the maintainer merges it.
  - Depends on: all tasks above
