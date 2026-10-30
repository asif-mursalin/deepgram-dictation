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
| The first run shows a permission dialog | _pending_ |
| Typing works in gedit | _pending_ |
| Typing works in Firefox | _pending_ |
| Typing works in GNOME Terminal | _pending_ |
| The second run skips the dialog (restore token) | _pending_ |
| The dialog is still skipped after a reboot (spec open question 2) | _pending_ |
| The active layout is detected: English (US) active | _pending_ |
| The active layout is detected: Arabic active (spec open question 4) | _pending_ |
| English text with Arabic active comes out as English | _pending_ |
| Time to connect, and time to type 13 characters | _pending_ |

**Conclusion:** _pending_
