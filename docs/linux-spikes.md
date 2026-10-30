# Linux spikes: results

Small test programs on the maintainer's machine (CachyOS, GNOME Shell 51.1, Wayland) answer the open questions in [`SPEC.md`](../SPEC.md) before the real code is written. The tasks are S1–S4 in [`tasks/todo.md`](../tasks/todo.md). The programs are in [`spikes/`](../spikes).

## S1: typing through the RemoteDesktop portal and libei

Program: `spikes/typing` (uses `eitype` 0.2.2, which builds on `reis` 0.5 and `ashpd` 0.12).

```bash
cd spikes/typing && cargo build --release
./target/release/typing-spike                 # types "Hello, world." after 5 s
./target/release/typing-spike --forget        # delete the token, see the dialog again
./target/release/typing-spike --layout 0 Hi   # force the first layout (English)
```

| Question | Result |
|---|---|
| The first run shows a permission dialog | Yes. Approved in about 7 s; the token was saved with mode 0600 |
| Typing works in gedit | Yes, on the second try (see the notes below) |
| Typing works in Firefox | Yes (address bar) |
| Typing works in GNOME Terminal | Yes (at the prompt) |
| The second run skips the dialog (restore token) | Yes. Connected in 0.1 s, no dialog. GNOME returns a new token each time, which must be saved |
| The dialog is still skipped after a reboot (spec open question 2) | Yes. After a restart, it connected in 0.2 s with no dialog, and the text appeared |
| The active layout is detected: English (US) active | Yes, group 0 `us`. Sometimes through libei (the modifiers event), sometimes not; then eitype reads it from GNOME directly. The libei keymap has both layouts |
| The active layout is detected: Arabic active (spec open question 4) | Yes, group 1. With the automatic layout, eitype refuses: `Character not found in keymap: H`, and nothing is typed |
| English text with Arabic active comes out as English | **No.** Forcing layout 0 (`--layout 0`) presses the English key positions, but GNOME applies the active Arabic layout: "Hello, world." came out as `أثممخو صخقمي`. The app must not force a layout |
| Time to connect, and time to type 13 characters | 0.1 s with a token. Typing returns at once; the keys are sent without a delay |

**Notes:**

- **First try failed: the program quit too soon.** It typed and closed the connection at once, and nothing appeared. With 1 s of waiting after the last key, the text appears. An idle connection is fine: `--idle 20` (connected, silent for 20 s, then typing) worked. Only 20 s was tested; the real app keeps the connection for hours, so T8 must reconnect when a send fails.
- **The layout is read only once, at connect.** In one run, Arabic was active at connect, and the user switched to English during the idle time. The program still used the Arabic layout and refused to type. The app must check the active layout before each dictation, not once at start.
- **How eitype finds the layout varies.** Sometimes GNOME reports the group through libei (the modifiers event), sometimes not, and then eitype reads it from GNOME directly. Both gave the right answer in every run.

**Conclusion:** typing through the RemoteDesktop portal and libei works on GNOME 51, in gedit, Firefox and GNOME Terminal. The permission is asked once and then remembered, also after a reboot. Release 1 can go ahead as planned (Checkpoint A, S1). For T8:

1. Keep one connection open, and do not close it right after typing.
2. Before each dictation, read the active layout again.
3. If any character cannot be typed on the active layout (for example, Arabic is active), do not type: paste the whole text once from the clipboard (S2 decides how).
4. Never force a layout: GNOME applies the active layout to the keys, so English text comes out as Arabic letters.
5. Save the new restore token after every connection (mode 0600).

## S2: the clipboard from the background, and pasting

Program: `spikes/clipboard` (`ashpd` 0.12.3 for the portals, `arboard` 3.7 for X11).

```bash
cd spikes/clipboard && cargo build --release
./target/release/clipboard-spike portal          # set the clipboard through the portal; you press Ctrl+V
./target/release/clipboard-spike portal --paste  # same, and the program presses Ctrl+V
./target/release/clipboard-spike x11 --paste     # the X11 clipboard through XWayland
./target/release/clipboard-spike keysym          # no clipboard: type each character as a keysym
```

| Question | Result |
|---|---|
| Portal: the clipboard can be set from the background | Yes. The first run shows the Remote Desktop dialog with two switches, "Allow Remote Interaction" and "Allow Clipboard Access"; both must be on. Apps read the text as `text/plain;charset=utf-8` |
| Portal: the program's Ctrl+V pastes it (English active) | Yes, in gedit |
| Portal: Ctrl+V pastes it with Arabic active | Yes: the English text is pasted in English. The first try showed no paste, most likely because the click into gedit came too late; the second try worked, and the log shows gedit reading the clipboard after Ctrl+V |
| X11 through XWayland: the clipboard can be set from the background | Yes (`DISPLAY=:0`, arboard) |
| X11: the program's Ctrl+V pastes it in a Wayland app (gedit) | Yes. Mutter shares the X11 clipboard with Wayland apps |
| Keysym: English text comes out as English with Arabic active | **Yes.** `NotifyKeyboardKeysym` through the RemoteDesktop portal typed "Hello … test." in English while Arabic was active. GNOME finds the key on any configured layout (unlike libei keycodes in S1) |
| Keysym: characters that are on no layout (é, —) are typed | **No.** They are dropped without an error: "café —" came out as "caf " |
| The "do not keep in history" hint | **Ignored.** The maintainer's Clipboard History extension (`clipboard-history@alexsaveau.dev`) reads `text/plain` right away and keeps the entry; it never asks for `x-kde-passwordManagerHint`. KDE's Klipper honors the hint (not tested here) |

**Conclusion:** the Clipboard portal works on GNOME 51, through the same RemoteDesktop session that types. It also pastes correctly with Arabic active. Open question 1 in SPEC.md is answered: the clipboard copy is possible. The X11 clipboard through XWayland works too, as a backup.

The keysym test changes the plan for typing (T8):

1. **Type with keysyms, not libei keycodes.** `NotifyKeyboardKeysym` through the RemoteDesktop portal types English correctly with Arabic active. libei keycodes (S1) did not. This needs no `reis` and no `xkbcommon`.
2. **Paste when a character is on no layout.** Keysyms drop such characters (é, —, emoji) without an error, so the app must decide before it types. For release 1 (English dictation, US and Arabic layouts): if every character is printable ASCII, a space or a new line, type it; otherwise paste the whole dictation once with the portal clipboard and Ctrl+V.
3. **One clipboard entry per dictation.** History tools may ignore the hint, so set the clipboard only once per dictation: the final copy, which is also the paste when the fallback is used.
4. **Ask for both switches.** The first-start dialog must have "Allow Remote Interaction" and "Allow Clipboard Access" on. `docs/linux.md` must say so.
5. **Speed not measured.** Each keysym is one D-Bus call (press and release). T8 measures a long dictation (500 characters); if it is too slow, use the clipboard paste for long texts.

## S3: Alt+M through the GlobalShortcuts portal

Program: `spikes/hotkeys` (`ashpd` 0.12.3).

```bash
cd spikes/hotkeys && cargo build --release
./target/release/hotkeys-spike                       # no app ID
./target/release/hotkeys-spike --app-id io.github.rawasmhd.DeepgramDictation
```

| Question | Result |
|---|---|
| Without an app ID: GNOME accepts the request | _pending_ |
| What GNOME 51 shows the first time | _pending_ |
| Alt+M, Ctrl+Alt+Z, Ctrl+Alt+Q are granted as asked | _pending_ |
| Presses arrive (and releases) | _pending_ |
| The second run asks again | _pending_ |
| With an app ID: the same questions | _pending_ |
| Alt+M still reaches other apps while bound | _pending_ |

**Conclusion:** _pending_
