# Spec: Linux version of deepgram-dictation

Status: draft for review (2026-10-29). Tracking issue: to be created.

## Objective

Bring the dictation app to Linux, so that it works the way it does on Windows: press **Alt+M**, speak, press **Alt+M** again, and the text appears at the cursor in any app.

**Users:** people on a Linux desktop who already use, or want, push-to-talk dictation with their own Deepgram key. First target: the maintainer's own machine (CachyOS, GNOME 51, Wayland).

**Supported desktops, in order:**

1. GNOME on Wayland, version 48 or later (needs the GlobalShortcuts and RemoteDesktop portals).
2. KDE Plasma 6 on Wayland. It uses the same portals, so it should work almost for free.
3. Later, not in this spec: X11 sessions, Sway, Hyprland.

**The Windows app does not change** in what it does. Shared code moves into a library that both apps use (see Project Structure).

### Release 1: the core (Linux 0.1)

- **Alt+M** starts and stops a dictation, through the XDG GlobalShortcuts portal. The first start shows GNOME's dialog, where the user confirms the key or picks another.
- `dictation --toggle` does the same from a terminal or from a shortcut set in the desktop settings. This is the fallback for desktops without the portal.
- **Ctrl+Alt+Q** quits.
- Streaming transcription with Deepgram, with the batch fallback, exactly as on Windows (shared code).
- **Text at the cursor:** the app types the text into the window that has focus, through the RemoteDesktop portal with libei. The first start asks for permission once. The app saves the restore token, so it does not ask again.
- **Keyboard layouts:** the maintainer uses English (US) and Arabic. Before it types, the app checks that every character can be typed on the active layout. If one cannot (for example, Arabic is active while dictating English, or the text has an emoji), it pastes the whole dictation once from the clipboard with Ctrl+V. It never types English text as Arabic letters.
- **Clipboard:** one copy of the full dictation goes on the clipboard at the end, as on Windows, so Ctrl+V pastes it again. Phrases are typed, not pasted, so the clipboard history gets at most one entry per dictation. If GNOME does not let a background app set the clipboard, the text is only typed (see Open Questions, item 1).
- **Undo (Ctrl+Alt+Z):** works as on Windows: it deletes the last dictation with Backspace. On GNOME, the app cannot see that the user typed or switched windows after the dictation, so undo cannot cancel itself there. The README must say this clearly. (Decision of the maintainer: same scope as Windows.)
- **Custom words:** `words.txt`, shared code, same behavior.
- **Tray icon** (StatusNotifierItem) with the state dot and a menu: Start dictation, Settings (opens the config file), Custom words (opens `words.txt`), Start at login, Open log file, Quit. Without a tray (stock GNOME without the AppIndicator extension), the app still works, and it shows desktop notifications for errors.
- **Settings without a window:** the API key and settings are in `~/.config/dictation/.env`. On the first start without a key, the app asks for it in the terminal if there is one. Otherwise it shows a notification that says where to put it.
- **Start at login:** a `.desktop` file in `~/.config/autostart/`.
- **Only one copy runs:** a lock file in `$XDG_RUNTIME_DIR`. A second start with `--toggle` sends the toggle to the running copy over a Unix socket.
- **Log:** `~/.local/state/dictation/dictation.log`.

### Release 2: parity (Linux 0.2)

- **Floating meter** with the level, timer and the "Transcribing" animation. The maintainer wants both the meter and the tray icon. First try an X11 window through XWayland (override-redirect, ARGB, click-through). If GNOME does not keep it on top, build a small GNOME Shell extension that draws the meter, fed by the app over D-Bus (Open Questions, item 3).
- **Setup window** and **Custom words window** in GTK4 with libadwaita. They run as short-lived processes (`dictation --setup`, `dictation --words`), so the running app never loads GTK and stays small.
- **Live paste:** each phrase is typed while the user speaks. On GNOME it goes to whatever window has focus, because the app cannot see which window had focus at the start. No "Paused" state on GNOME.
- **Light and dark mode** from the Settings portal (`color-scheme`), for the meter and the tray icon.

### Out of scope

- X11, Sway, Hyprland, other compositors (later issues).
- A GNOME Shell extension, unless the meter test needs one (Release 2).
- Flatpak, AppImage, `.deb` and `.rpm` (later issues).
- macOS (#1).
- Changes to what the Windows app does.

## Tech Stack

| Part | Choice | Notes |
| --- | --- | --- |
| Language | Rust, edition 2021 | as today |
| Audio | `cpal` 0.15 (ALSA, through PipeWire's ALSA plugin) | shared; the upgrade to 0.18 is a separate issue |
| Deepgram | `tungstenite` 0.24, `ureq` 2 | shared |
| TLS | Linux: `rustls` with the `ring` provider. Windows: unchanged (`native-tls`, SChannel) | no OpenSSL at runtime on Linux; moving Windows to rustls is a separate, later issue |
| Hotkeys | `ashpd` 0.13 (GlobalShortcuts) | Linux only |
| Typing | `ashpd` (RemoteDesktop) + `reis` (libei) + `xkbcommon` (character to keycode) | Linux only; check the latest versions when the task starts |
| Tray | `ksni` 0.3 (`blocking` feature, no tokio) | pure Rust over D-Bus, no GTK |
| Notifications | `notify-rust` 4 | Linux only |
| Paths | `dirs` | XDG config and state folders |
| Windows (release 2) | `gtk4` 0.11 + `libadwaita` 0.9 | only in the `--setup` and `--words` processes |
| Meter (release 2) | `x11rb` 0.14 | through XWayland; a GNOME Shell extension (JavaScript) if that fails |

Adding any other crate needs the maintainer's approval first.

## Commands

The shared library and the Linux app each build on their own, like `rust/` today.

```bash
# Build-time packages (CachyOS / Arch)
sudo pacman -S --needed alsa-lib pkgconf libxkbcommon
# Build-time packages (Ubuntu, also in CI)
sudo apt-get install -y pkg-config libasound2-dev libxkbcommon-dev

# Shared library: tests (Linux and Windows)
cd core && cargo test

# Linux app
cd linux && cargo build              # debug build, logs to the terminal
cd linux && cargo build --release    # target/release/dictation
cd linux && cargo test

# Windows app (unchanged)
cd rust && cargo build --release
cd rust && cargo test

# Install the desktop file and icons for development (portals need the app ID)
./linux/target/release/dictation --install-desktop

# Start the Linux app the way the desktop does, so the portals see the app ID
gtk-launch io.github.rawasmhd.DeepgramDictation
```

A build must have no warnings (as in CONTRIBUTING.md).

## Project Structure

As the maintainer asked, the Linux app goes in its own folder. The code that both apps use moves into a shared library, so it is not copied twice.

```text
core/                      → NEW: library crate `dictation-core`, no Windows or Linux calls
  src/audio.rs             ← moved from rust/src (cpal)
  src/deepgram.rs          ← moved
  src/words.rs             ← moved
  src/gfx.rs, art.rs       ← moved (build.rs uses them too)
  src/session.rs           ← moved; HWND becomes a generic `WindowId`
  src/live.rs              ← the portable part (what to paste, and when)
  src/env.rs               ← the portable part of config.rs (.env and key parsing)
rust/                      → the Windows app (folder name unchanged, so the CI and
                             Explorer paths in CLAUDE.md stay valid); depends on ../core
linux/                     → NEW: the Linux app, binary `dictation`; depends on ../core
  src/main.rs              → start, single instance, event loop, Host impl
  src/hotkey.rs            → GlobalShortcuts portal
  src/typing.rs            → RemoteDesktop portal + libei: type text, Backspace
  src/clipboard.rs         → clipboard (approach decided by Open Questions, item 1)
  src/tray.rs              → ksni tray
  src/paths.rs             → XDG folders
  src/autostart.rs         → ~/.config/autostart
  src/control.rs           → lock file and Unix socket for --toggle
  src/overlay.rs           → release 2: the meter
  src/setup.rs, words_window.rs → release 2: GTK4 windows
  data/                    → .desktop file, metainfo; icons are rendered from core/art.rs
docs/linux.md              → NEW: install, permissions, what differs from Windows
```

Each crate has its own `Cargo.toml`, `Cargo.lock` and `target/`. There is no workspace at the root, so `rust/target/release/dictation.exe` stays where it is.

**App ID:** `io.github.rawasmhd.DeepgramDictation`. It is used for the `.desktop` file, the icons and the portals.

## Code Style

Match the existing code: small functions, plain constants, a `//!` header on each file that says what it does, and a short comment where the reason is not obvious. Errors are `Result<_, String>` with a message a user can read. No `unwrap()` on anything that can fail at runtime.

Platform code implements the existing `Host` trait from `session.rs`. The state machine is not copied:

```rust
//! Typing into the focused window, through the RemoteDesktop portal and
//! libei. GNOME asks for permission once; the restore token in
//! ~/.config/dictation/portal-token skips the dialog after that.

/// Type `text` at the cursor. Characters that the keyboard layout cannot
/// type go through the clipboard and Ctrl+V.
pub fn type_text(&mut self, text: &str) -> Result<(), String> {
    for c in text.chars() {
        match self.keymap.keycode(c) {
            Some(key) => self.tap(key)?,
            None => self.paste_char(c)?,
        }
    }
    Ok(())
}
```

Linux-only code stays in `linux/`. `core/` never imports `windows-sys`, `ashpd` or any other platform crate.

## Testing Strategy

- **Shared logic (`core/`):** the existing unit tests move along with the code (session, deepgram, words, audio, gfx, art, live) and run on Linux and Windows in CI. Session tests keep using the fakes.
- **Linux unit tests (`linux/`, `#[cfg(test)]` in each file):** character to keycode mapping, XDG paths, the autostart file, the `--toggle` socket protocol, and restore-token save and load. These tests do not need a desktop.
- **No tests that open a portal or a window in CI.** Portals need a real desktop session.
- **Manual checklist on GNOME 51 Wayland**, run before each Linux release and written in `docs/linux.md`:
  1. First start: the shortcut dialog and the remote-input dialog appear. After a restart, the remote-input dialog does not appear again.
  2. Alt+M → speak → Alt+M types the text into gedit, Firefox, and GNOME Terminal.
  3. Ctrl+Alt+Z deletes the last dictation.
  4. `dictation --toggle` works while the app runs.
  5. Tray: each menu item works. Turning start-at-login on and off adds and removes the autostart file.
  6. Without a network: a clear notification, and no crash.
  7. Memory of the running app below 30 MB (`ps -o rss`).
- **Benchmark:** run `bench/` on Windows before and after the move to `core/`, so the timings do not get worse.
- **CI:** a matrix of `windows-latest` and `ubuntu-24.04`. Each runs `cargo build --release --locked` and `cargo test --locked` for the crates that apply.

## Boundaries

**Always:**

- Give each change a task in `tasks/todo.md` (in the shape of the task template) before work starts. Post the matching GitHub issues before the release pull request into `main` (see Branches and tracking).
- Keep the Windows app building and passing its tests after every step.
- Run `cargo build` (no warnings) and `cargo test` in each changed crate before a commit.
- Store the API key with file mode 0600.
- Start the Linux app with `gtk-launch` (or from the desktop), not as a child of a Claude Code shell. (This is the Linux version of the Explorer rule, so the app ID is correct and the app survives the session.)

**Ask first:**

- Adding a crate that is not in the Tech Stack table.
- Changing CI or release workflows beyond adding the Linux job.
- Any change to what the Windows app does.
- Editing shared files: `README.md`, `CLAUDE.md`, `.github/CONTRIBUTING.md`, and `rust/Cargo.toml`. Run these one after the other, not in parallel.
- Anything that needs root, a udev rule, or the `input` group (uinput, evdev).

**Never:**

- Commit `.env`, API keys or restore tokens.
- Read or store which keys the user presses.
- Put Windows or Linux calls in `core/`.
- Delete or skip failing tests without approval.

## Success Criteria

**Release 1 is done when:**

1. On GNOME 51 Wayland, Alt+M → speak → Alt+M types the transcript into gedit, Firefox and GNOME Terminal.
2. After the first-run dialogs, a restart of the app (and a reboot) asks for no permission again. If GNOME forgets the token after a reboot, it asks once and keeps working.
3. Ctrl+Alt+Z deletes the last dictation. Ctrl+Alt+Q quits. `dictation --toggle` works.
4. The tray shows idle, recording, transcribing and problem, and every menu item works.
5. Start at login works after a reboot.
6. The running app uses less than 30 MB of memory.
7. CI builds and tests both Windows and Linux. The Windows tests that existed before still pass.
8. A tag publishes `dictation-linux-x86_64.tar.gz` (binary, `.desktop`, icons, `install.sh`) next to `dictation.exe`, and an AUR `-bin` package installs it.
9. `docs/linux.md` covers install, the permission dialogs, the tray extension, and the differences from Windows.

**Release 2 is done when:**

1. The meter shows above other windows while recording on GNOME Wayland, next to the tray icon. It never takes focus or catches a click.
2. `dictation --setup` and `--words` open GTK4 windows that save the key (after checking it with Deepgram) and `words.txt`.
3. Live paste types each phrase while the user speaks.
4. The meter and icons follow the light or dark setting.

## Planned order (detailed in tasks/plan.md after approval)

0. Groundwork:
   - Move the shared code into `core/`. The Windows app uses it, and nothing else changes.
   - Give the Linux app rustls (Windows keeps SChannel).
   - Add Linux to CI.
1. A Linux skeleton: paths, logging, `.env`, single instance, `--toggle`, tray, record and transcribe, and print the text to the log.
2. Typing through the RemoteDesktop portal, then undo.
3. The GlobalShortcuts portal, autostart, notifications.
4. Packaging: tarball, AUR, `docs/linux.md` → **release 1**.
5. Meter test (XWayland, else a GNOME Shell extension), then the meter, GTK windows, live paste, dark mode → **release 2**.

## Branches and tracking

The Linux work has a root branch per release. Task branches merge into it, and it merges into `main` once, at the release.

**The work happens in a fork.** The maintainer's GitHub account `asif-mursalin` has no write access to `rawasmhd/deepgram-dictation`, so:

- `origin` is the fork, `asif-mursalin/deepgram-dictation`. The root branch, the task branches and their pull requests live there.
- `upstream` is `rawasmhd/deepgram-dictation`. Nothing is pushed to it. Its `main` is only fetched to sync.
- The release pull request goes from `asif-mursalin:feature/linux-r1` into `rawasmhd:main`.
- CI runs in the fork. GitHub Actions must be turned on once in the fork's **Actions** tab.

```text
main ──●──────────●─────────────────●──────────────●──→  Windows releases, as today
        \          \ sync            ↑ release 1    ↑ release 2
feature/linux-r1                     │ (after       │
         ●──●──●────●──●──●──●──────●  regression   │
            ↑  ↑       ↑  ↑            tests pass)  │
            task branches (feature/<task>-<name>)   │
                                      \             │
                      release 2 root   ●──●──●──●───●
```

1. **`feature/linux-r1`** is the root branch for release 1. It starts from the latest `main`. It holds all the release 1 work, including the move of the shared code into `core/`.
2. **Task branches** start from `origin/feature/linux-r1` (the fork) and are named `<type>/<task>-<short-name>`, for example `feature/T3-typing`. Their pull requests go into the root branch, inside the fork. Delete each one after its merge.
3. **Sync often.** The `core/` move changes Windows files, so run `git fetch upstream` and merge `upstream/main` into the root branch each time `rawasmhd`'s `main` changes. Also push it to the fork's `main` (`git push origin upstream/main:main`), so both stay the same. Do it right away if the change touches `rust/src`. Then run `cargo build` and `cargo test` in `core/`, `rust/` and `linux/`.
4. **Nothing goes to `main` until the regression tests pass.** The release pull request from the root branch into `main` is merged only when all of these pass on the root branch:
   - CI: `cargo build --release --locked` and `cargo test --locked` for `core/`, `rust/` and `linux/`, on Windows and Linux.
   - Windows regression: the Windows app does everything it did before the branch. Check Alt+M, live paste, the Paused state, Ctrl+Alt+Z, the meter, the tray menu, Settings, Custom words, start at login, and Ctrl+Alt+Q in the Windows 11 VM (W1 in `tasks/todo.md`). Run `bench/` and compare the timings with the last result in `bench/results/`.
   - Linux: the manual checklist in Testing Strategy, and the release success criteria.
5. **Release 1:** after the regression tests pass, one pull request from the fork merges the root branch into `rawasmhd:main`. The Linux release is tagged from `main`. After that, delete the root branch.
6. **The release 2 root branch** starts from `main` after release 1 is merged, and works the same way. Its name is chosen then.
7. **CI** runs on pushes to the root branch too, not only to `main`. In the fork, that needs Actions turned on.

**Tracking:** until the issues are posted, the tasks are tracked in `tasks/todo.md` on the root branch. Task names are `T1`, `T2`, and so on. Before the release pull request into `main`, post the tracking issue and one issue per task in `rawasmhd/deepgram-dictation`. The release pull request links them (`Closes #…`).

## Decisions (2026-10-29)

- Linux app in its own folder (`linux/`), with the shared code in `core/`.
- Undo: same scope as Windows. On GNOME it cannot cancel itself after typing; the docs say so.
- Clipboard: one copy of the full dictation per dictation. No clipboard copy per phrase.
- Meter and tray icon both, on GNOME too.
- Keyboard layouts: English (US) and Arabic. Fall back to one clipboard paste when the active layout cannot type the text.
- Windows keeps its current TLS (SChannel). The official `.exe` is built by CI with MSVC.
- One root branch per release. Release 1 uses `feature/linux-r1`. The `core/` move goes into it, not straight into `main`.
- Nothing merges into `main` until the regression tests pass (Branches and tracking, item 4).
- Tasks are tracked locally in `tasks/todo.md` until the release. Issues are posted before the release pull request.

## Open Questions

These are answered by short tests on the maintainer's machine, during the work. Each has a decided fallback.

1. **Clipboard on GNOME.** Try the portal Clipboard interface, then the X11 clipboard through XWayland. If neither works: type only, no clipboard copy. The layout fallback then cannot work either, so the app tells the user to switch to English (US) when the text cannot be typed.
2. **Permission after a reboot.** Answered by S1: yes, GNOME keeps it. The dialog appears only on the first start (`docs/linux-spikes.md`).
3. **The meter on GNOME.** Does GNOME keep an XWayland override-redirect window above other windows? If not: a GNOME Shell extension.
4. **Active layout under libei.** Answered by S1: yes, but only at the moment it asks. The app checks it before each dictation. With Arabic active, it pastes from the clipboard; forcing English would type Arabic letters (`docs/linux-spikes.md`).
