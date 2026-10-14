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
- Closed tabs and unavailable machines are skipped, including the machine
  you are on, local or remote, while it is disconnected or reconnecting. Tabs from before a machine's Herdr server
  restarted are skipped too, because a restarted server reuses tab IDs for
  new tabs.
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

## Mark a tab or agent unread

Like marking an email unread, this fork lets you put the "needs attention" dot
back on something you glanced at but haven't dealt with yet.

- Right-click a tab in the tab bar and choose **Mark unread** to mark the
  whole tab. This works for plain shells with no agent too.
- Right-click an agent in the sidebar's agent list and choose **Mark unread**
  to mark just that agent. This works for agents on remote machines too.
- A marked tab or agent shows the `done` status (the teal dot) on its agent
  rows, its workspace in the sidebar, and its tab. Tabs you marked from the
  tab menu also get a teal `●` before their name in the tab bar. Official
  Herdr has no dots in the tab bar, and this fork adds one only for tabs you
  marked; marking an agent from the sidebar doesn't add one.
- The mark clears the next time you view it. A tab mark clears when you view
  the tab, even if it is zoomed to one pane. An agent mark clears when that
  agent's pane is on screen. If you mark what you're looking at, it stays
  marked until you switch away and come back. Switching to another machine
  counts as switching away. Marks don't clear while the terminal window itself
  is unfocused, or before the screen has actually been redrawn for the view.
- Marking never switches to the tab. Blocked agents keep their more urgent
  red status while marked.
- A mark only changes what you see. Herdr's check that an agent really
  finished before showing a "finished" notification ignores marks, so marking
  a working agent doesn't let a stale notification or sound through.
- Marks are kept in the TUI client's memory, per machine. They are not shared
  with other clients, are lost when the client exits, and are dropped when a
  machine's Herdr server restarts.

## Implementation and coverage

### Mark unread

Marks live in `src/client/shell/unread_marks.rs`, one set per endpoint, beside
the existing client-side `done` tracking. Tab marks and agent (pane) marks are
kept separately. A mark is cleared only by a pane surface whose boot ID and
projection revision match the cached snapshot, the same guard the existing
`done` acknowledgement uses, so a surface received ahead of its snapshot can't
clear a mark the user hasn't seen. Each server snapshot is projected
with the marks applied, and the statuses they replaced are restored before the
next projection, so unmarking a plain-shell tab returns it to its original
status. No server, API, or wire-protocol changes were needed. Unit tests cover
agent, tab, and workspace projection, plain shells, clearing on view, zoomed
tabs, the "marked while visible" case, leaving a machine, blocked agents,
closed panes and tabs, the surface guard, and the status a mark replaced. Shell tests drive the tab and
agent right-click menus, check that marking does not focus the tab, that marks
survive new server snapshots, that only tabs marked from the tab menu get
the tab-bar dot, that marking a working agent doesn't validate a "finished"
notification, and
that a surface ahead of the snapshot does not clear a mark. Validated with
`cargo fmt`, Clippy with warnings denied, and the full unit suite. Running the unit suite in one parallel pass exits with
SIGPIPE in `server::headless` tests on this machine, on unmodified `main`
too, so those 187 tests were run single-threaded, where they pass.

### Tab history

History belongs to the TUI client and uses the existing endpoint activation and
tab focus actions. Entries include both the machine identity and tab identity.
Entries also record the server's boot ID and are only offered while it
matches. During a cross-machine jump, temporary tab views are ignored while the
target settles; pending navigation expires after three seconds, or ends at
once if the focus request could not be sent.

The added tests cover back/forward traversal, repeated navigation before a jump
lands, forward-history clearing, transient views, failed-jump timeout,
deduplication, unavailable entries, cancelled jumps, and the history size
limit. Endpoint tests check switching between local and remote machines with
the same tab ID using the configured keyboard shortcuts, skipping tabs from
before a server restart, and skipping disconnected local and remote machines.

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

[AGENTS.md](AGENTS.md) requires agents to update this file, and the fork
summary at the top of [README.md](README.md), whenever they change the fork's
code, configuration, documentation, or repository workflow. It also
requires fetching official Herdr and reporting commits ahead and behind before
starting changes and before committing or pushing. Catching up with upstream
requires a separate user request.
