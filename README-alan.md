# herdr-alan

Alan Rockefeller's fork of [Herdr](https://github.com/herdrdev/herdr), a terminal
workspace manager for coding agents. This document describes the additions in
this fork. See [README.md](README.md) for the original project's overview and
[LICENSE](LICENSE) for its Apache 2.0 license.

## Back and forward through viewed tabs

This fork adds browser-style back and forward navigation through recently
viewed tabs. History spans workspaces and connected machines, so you can return
to a tab on another machine without finding it again in the navigator.

- `history_back` returns to a previously viewed tab.
- `history_forward` moves forward after going back.
- Tabs on different machines remain distinct even when their tab IDs match.
- Closed tabs and unavailable machines are skipped.
- Rapidly cycling through tabs filters out intermediate visits shorter than
  750 milliseconds. Revisiting a tab moves it to the newest position instead
  of keeping duplicate entries.
- Visiting a different tab after going back clears the forward history.
- Each TUI client keeps up to 100 history entries in memory. History is not
  saved between client sessions.

Both actions are unset by default. To enable them, add these bindings to the
existing `[keys]` section of your Herdr configuration:

```toml
[keys]
history_back = "alt+l"
history_forward = "alt+k"
```

On Linux and macOS, the usual config path is `~/.config/herdr/config.toml`;
on Windows, it is `%APPDATA%\herdr\config.toml`. These shortcuts are examples
and can be changed. The actions also appear in keybinding help, the generated
default configuration, and the draft configuration reference.

## Implementation and coverage

History belongs to the TUI client and uses the existing endpoint activation and
tab focus actions. Entries include both the machine identity and tab identity.
During a cross-machine jump, temporary tab views are ignored while the target
settles; pending navigation expires after three seconds.

The added tests cover back/forward traversal, repeated navigation before a jump
lands, forward-history clearing, transient views, failed-jump timeout,
deduplication, unavailable entries, and the history size limit. An endpoint test
checks switching between local and remote machines with the same tab ID using
the configured keyboard shortcuts.

### Validation at the initial fork commit

Formatting, Clippy with warnings denied, and the maintenance, architecture,
documentation, and integration-asset checks passed on Linux. The Rust suite ran
4,002 tests: 4,000 passed, including the new history tests, and two existing
integration tests failed:

- `machine_api_recovers_a_stale_path_before_sending_a_mutation`: the remote
  API bridge could not find its test session socket.
- `server_survives_hangup_and_logs_why_it_stops`: the expected server shutdown
  message did not appear in the test log.

The full check is therefore not green. Windows cross-lint also requires a
Windows SDK that was not configured on the validation machine. Tests used
`GIT_CONFIG_GLOBAL=/dev/null` to keep personal Git ignore rules out of their
disposable repository fixtures.

## Fork maintenance

[AGENTS.md](AGENTS.md) requires agents to update this file whenever they change
the fork's code, configuration, documentation, or repository workflow. It also
requires fetching official Herdr and reporting commits ahead and behind before
starting changes and before committing or pushing. Catching up with upstream
requires a separate user request.
