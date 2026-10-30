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
