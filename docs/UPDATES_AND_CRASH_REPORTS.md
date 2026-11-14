# Updates, crash reports and crash recovery (kimo desktop app)

Three safety features of `crates/kimo-gui`, all on Windows and Linux.

## In-app updates (Tauri updater, GitHub releases)

**What the app does.** A release build checks once, about 8 seconds after start, for
`https://github.com/naman-sharma12345/kimo/releases/latest/download/latest.json`. When a newer version is listed, an
**Update to x.y.z** button appears in the top bar. Nothing installs until the person clicks it and confirms. Then kimo:

1. downloads the installer and checks its signature against the public key built into the app,
2. autosaves the open project with its undo history,
3. stops the engine and agents (the installer replaces their files),
4. installs (Windows: passive NSIS installer, which reopens kimo; Linux: replaces the AppImage, or the .deb via
   `pkexec`) and restarts.

Unsaved work is offered back after the restart, the same way as after a crash.

- Debug builds never check. A build whose config has an empty `pubkey` reports `"state": "off"` in `app_state().update`.
- `settings.json` → `"update_check": false` turns the automatic check off. `update_check` (command) checks on demand.
- The check result is in `app_state().update`: `unchecked | off | none | available | error`.
- Only a **published** release is "latest". The release workflow makes drafts, so an update reaches users when a
  maintainer publishes the draft.

**The signing key.** The key pair was made with `tauri signer generate` (Tauri CLI 2.12.3) on 2026-11-14, on the
maintainer's PC, in `%USERPROFILE%\.tauri\`:

| File | What it is | Where it goes |
|---|---|---|
| `kimo-updater.key.pub` | public key: checks signatures | committed, `crates/kimo-gui/tauri.conf.json` → `plugins.updater.pubkey` |
| `kimo-updater.key` | private key: signs releases | GitHub secret `TAURI_SIGNING_PRIVATE_KEY` (the file's content). Never in the repo. |
| `kimo-updater.password.txt` | the private key's password | GitHub secret `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Never in the repo. |

Back up the private key and password somewhere safe (a password manager), not only on that PC. If they are lost,
installed copies can never update again and people must reinstall by hand. If they leak, anyone could ship a
"kimo update": rotate by generating a new pair, committing the new public key, and releasing that build. Only copies
that already have the new key can take updates signed with it.

`the_built_in_update_key_verifies_release_signatures` (kimo-gui, `src/lib.rs`) checks that a file signed with the
private key (`crates/kimo-gui/tests/updater_key/`) verifies with the committed public key, the way the updater checks
a download. To re-sign after rotating:

```bash
npx @tauri-apps/cli@2.12.3 signer sign -f ~/.tauri/kimo-updater.key -p <password> crates/kimo-gui/tests/updater_key/sample.txt
```

With the two secrets set, `.github/workflows/release.yml`:

- turns on `createUpdaterArtifacts` (the public key already comes from `tauri.conf.json`),
- copies each installer's updater signature next to it under the stable name + `.sig`
  (`kimo-setup-windows-x64.exe.sig`, `kimo-linux-x86_64.AppImage.sig`, `kimo-linux-amd64.deb.sig` when the CLI makes
  one),
- has the `updater` job write `latest.json` (keys `windows-x86_64[-nsis]`, `linux-x86_64[-appimage]`,
  `linux-x86_64-deb`) pointing at the tag's assets, and attaches it to the draft release.

Without them, the installers are built as before, with no update files, and the workflow prints a warning.
The installer asset names do not change.

**Not done yet.**

- **Windows code signing (Authenticode)**: TODO in the `gui` job. It needs a code-signing certificate (OV/EV, or Azure
  Trusted Signing), then `bundle.windows.signCommand` (or `certificateThumbprint` + `timestampUrl`). Until then
  SmartScreen warns on first install. The updater signature is a separate thing and does not need it.
- `requireSignedVersion` (blocks an attacker from pairing a newer version number with an older, genuinely signed
  installer) is off. Turn it on in `plugins.updater` once every release users update from was signed by a Tauri CLI
  that records the version in the signature.

## Crash reports (opt-in)

- A panic hook (`src/crash.rs`, installed first thing in `main`) writes `<data>/crashes/crash-<unix ms>.log`: version,
  OS, thread, message, source location and backtrace. It runs with `panic = "abort"` too. It never uses the network.
- On the next launch the window asks: **kimo closed unexpectedly last time. Send a crash report?** It shows the first
  lines of the log.
  - **Send report** opens a pre-filled GitHub issue in the browser. The home folder is replaced by `~`. The person reads
    it and submits it themselves.
  - **Don't send** sends nothing.
  - Either way the log moves to `crashes/answered/` (newest 20 kept) and is not asked about again.
- `<data>` is `%APPDATA%\dev.kimo.app` on Windows and `~/.local/share/dev.kimo.app` on Linux (or `KIMO_DATA_DIR`).
- To try it end to end: run `kimo-gui --crash-test` (it panics at once, before any window), then start kimo normally.

## Crash recovery keeps Undo

Autosave (every minute after an edit, or after 20 edits) writes `<project>.kimo.autosave` (or
`<data>/autosave/untitled-<pid>.kimo`) **with history**. It calls `save_project with_history history_steps=30`, which
writes `<autosave>.history` next to it. Restore calls `load_project with_history restore`. The engine's `restore` flag
makes the load not an undo step of its own, so the first Ctrl+Z after a restore reverts the last edit made before the
crash. Discard deletes the autosave and its history.

Tests:

- `crates/kimo-core/tests/history_restore.rs` (engine)
- `recovery::tests` and `recovery::real_engine::undo_works_after_a_crash_restore` (app; runs when `KIMO_MCP_BIN` is set)
- `crash::tests`
