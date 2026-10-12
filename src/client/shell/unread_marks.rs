//! Panes the user marked unread, like marking an email unread.
//!
//! This is TUI presentation state kept per endpoint. A marked pane shows the
//! `done` status on its agent row, tab, and workspace until the user views it
//! again. Plain shells can be marked too, so their tab and workspace show it.

use std::collections::{HashMap, HashSet};

use crate::api::schema::AgentStatus;
use crate::protocol::ClientShellSnapshot;

#[derive(Clone, Debug, Default)]
pub(super) struct UnreadMarks {
    /// Marked pane IDs. `true` while the pane was visible when marked: it is
    /// only cleared after the user looks away and then views it again.
    marks: HashMap<String, bool>,
    /// Statuses replaced by `apply`, restored before the next projection.
    originals: Originals,
}

#[derive(Clone, Debug, Default)]
struct Originals {
    agents: HashMap<String, AgentStatus>,
    tabs: HashMap<String, AgentStatus>,
    workspaces: HashMap<String, AgentStatus>,
}

impl UnreadMarks {
    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.marks.is_empty()
    }

    pub(super) fn mark(
        &mut self,
        pane_ids: impl IntoIterator<Item = String>,
        visible: &HashSet<&str>,
    ) {
        for pane_id in pane_ids {
            let waiting = visible.contains(pane_id.as_str());
            self.marks.insert(pane_id, waiting);
        }
    }

    /// The user is no longer looking at this endpoint's panes.
    pub(super) fn leave_view(&mut self) {
        for waiting in self.marks.values_mut() {
            *waiting = false;
        }
    }

    /// Clears marks on visible panes the user has come back to. Returns whether any cleared.
    pub(super) fn acknowledge(&mut self, visible: &HashSet<&str>) -> bool {
        let before = self.marks.len();
        self.marks.retain(|pane_id, waiting| {
            if visible.contains(pane_id.as_str()) {
                *waiting
            } else {
                *waiting = false;
                true
            }
        });
        self.marks.len() != before
    }

    /// Forgets replaced statuses because `snapshot` came fresh from the server.
    pub(super) fn forget_originals(&mut self) {
        self.originals = Originals::default();
    }

    /// Puts back the statuses `apply` replaced.
    pub(super) fn restore(&mut self, snapshot: &mut ClientShellSnapshot) {
        let originals = std::mem::take(&mut self.originals);
        for agent in &mut snapshot.agents {
            if let Some(status) = originals.agents.get(&agent.pane_id) {
                agent.agent_status = *status;
            }
        }
        for tab in &mut snapshot.tabs {
            if let Some(status) = originals.tabs.get(&tab.tab_id) {
                tab.agent_status = *status;
            }
        }
        for workspace in &mut snapshot.workspaces {
            if let Some(status) = originals.workspaces.get(&workspace.workspace_id) {
                workspace.agent_status = *status;
            }
        }
    }

    /// Shows marked panes, and their tabs and workspaces, as `done`.
    pub(super) fn apply(&mut self, snapshot: &mut ClientShellSnapshot) {
        let pane_ids: HashSet<&str> = snapshot
            .panes
            .iter()
            .map(|pane| pane.pane_id.as_str())
            .collect();
        self.marks
            .retain(|pane_id, _| pane_ids.contains(pane_id.as_str()));
        if self.marks.is_empty() {
            return;
        }
        let mut tabs = HashSet::new();
        let mut workspaces = HashSet::new();
        for pane in &snapshot.panes {
            if self.marks.contains_key(&pane.pane_id) {
                tabs.insert(pane.tab_id.as_str());
                workspaces.insert(pane.workspace_id.as_str());
            }
        }
        for agent in &mut snapshot.agents {
            if self.marks.contains_key(&agent.pane_id) {
                raise(
                    &mut agent.agent_status,
                    &agent.pane_id,
                    &mut self.originals.agents,
                );
            }
        }
        for tab in &mut snapshot.tabs {
            if tabs.contains(tab.tab_id.as_str()) {
                raise(&mut tab.agent_status, &tab.tab_id, &mut self.originals.tabs);
            }
        }
        for workspace in &mut snapshot.workspaces {
            if workspaces.contains(workspace.workspace_id.as_str()) {
                raise(
                    &mut workspace.agent_status,
                    &workspace.workspace_id,
                    &mut self.originals.workspaces,
                );
            }
        }
    }
}

/// Shows `done` unless the current status needs more attention.
fn raise(status: &mut AgentStatus, id: &str, originals: &mut HashMap<String, AgentStatus>) {
    if super::status_priority(*status) < super::status_priority(AgentStatus::Done) {
        originals.entry(id.to_owned()).or_insert(*status);
        *status = AgentStatus::Done;
    }
}

impl super::ClientShellState {
    /// Marks every pane in a tab on the active endpoint unread.
    pub(super) fn mark_tab_unread(&mut self, tab_id: &str) {
        let pane_ids = self
            .snapshot
            .as_deref()
            .map(|snapshot| {
                snapshot
                    .panes
                    .iter()
                    .filter(|pane| pane.tab_id == tab_id)
                    .map(|pane| pane.pane_id.clone())
                    .collect()
            })
            .unwrap_or_default();
        let endpoint_id = self.active_endpoint_id.clone();
        self.mark_panes_unread(&endpoint_id, pane_ids);
    }

    pub(super) fn mark_panes_unread(
        &mut self,
        endpoint_id: &crate::client::endpoint::ClientEndpointId,
        pane_ids: Vec<String>,
    ) {
        let active = endpoint_id == &self.active_endpoint_id;
        let visible: HashSet<&str> = self
            .pane_surface
            .as_ref()
            .filter(|_| active)
            .map(|surface| {
                surface
                    .panes
                    .iter()
                    .map(|pane| pane.pane_id.as_str())
                    .collect()
            })
            .unwrap_or_default();
        let Some(endpoint) = self
            .endpoints
            .iter_mut()
            .find(|endpoint| &endpoint.endpoint_id == endpoint_id)
        else {
            return;
        };
        endpoint.unread.mark(pane_ids, &visible);
        if let Some(snapshot) = endpoint.snapshot.as_deref_mut() {
            endpoint.unread.restore(snapshot);
            endpoint.unread.apply(snapshot);
        }
        if active {
            self.snapshot = endpoint.snapshot.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ClientShellAgent, ClientShellPane};

    fn pane(pane_id: &str, tab_id: &str) -> ClientShellPane {
        ClientShellPane {
            pane_id: pane_id.into(),
            workspace_id: "w1".into(),
            tab_id: tab_id.into(),
            label: None,
            cwd: None,
            foreground_cwd: None,
            focused: false,
            right_click_passthrough: false,
        }
    }

    fn fixture() -> ClientShellSnapshot {
        let mut snapshot = crate::client::shell::tests::snapshot();
        let mut tab = snapshot.tabs[0].clone();
        snapshot.tabs[0].tab_id = "t1".into();
        snapshot.tabs[0].workspace_id = "w1".into();
        snapshot.tabs[0].agent_status = AgentStatus::Idle;
        tab.tab_id = "t2".into();
        tab.workspace_id = "w1".into();
        tab.agent_status = AgentStatus::Unknown;
        snapshot.tabs.push(tab);
        snapshot.workspaces.truncate(1);
        snapshot.workspaces[0].workspace_id = "w1".into();
        snapshot.workspaces[0].agent_status = AgentStatus::Idle;
        snapshot.panes = vec![pane("agent", "t1"), pane("shell", "t2")];
        snapshot.agents = vec![ClientShellAgent {
            pane_id: "agent".into(),
            workspace_id: "w1".into(),
            tab_id: "t1".into(),
            name: None,
            display_agent: None,
            agent: Some("claude".into()),
            title: None,
            terminal_title: None,
            terminal_title_stripped: None,
            agent_status: AgentStatus::Idle,
            state_change_seq: 1,
            state_labels: Vec::new(),
            tokens: Vec::new(),
            focused: false,
        }];
        snapshot
    }

    fn statuses(
        snapshot: &ClientShellSnapshot,
    ) -> (AgentStatus, AgentStatus, AgentStatus, AgentStatus) {
        (
            snapshot.agents[0].agent_status,
            snapshot.tabs[0].agent_status,
            snapshot.tabs[1].agent_status,
            snapshot.workspaces[0].agent_status,
        )
    }

    #[test]
    fn marked_agent_shows_done_on_row_tab_and_workspace() {
        let mut marks = UnreadMarks::default();
        let mut snapshot = fixture();
        marks.mark(["agent".to_owned()], &HashSet::new());
        marks.apply(&mut snapshot);
        use AgentStatus::*;
        assert_eq!(statuses(&snapshot), (Done, Done, Unknown, Done));
    }

    #[test]
    fn marked_plain_shell_shows_done_on_its_tab() {
        let mut marks = UnreadMarks::default();
        let mut snapshot = fixture();
        marks.mark(["shell".to_owned()], &HashSet::new());
        marks.apply(&mut snapshot);
        use AgentStatus::*;
        assert_eq!(statuses(&snapshot), (Idle, Idle, Done, Done));
    }

    #[test]
    fn viewing_a_marked_pane_clears_it_and_restores_statuses() {
        let mut marks = UnreadMarks::default();
        let mut snapshot = fixture();
        marks.mark(["shell".to_owned()], &HashSet::new());
        marks.apply(&mut snapshot);
        marks.restore(&mut snapshot);
        assert!(marks.acknowledge(&HashSet::from(["shell"])));
        marks.apply(&mut snapshot);
        use AgentStatus::*;
        assert_eq!(statuses(&snapshot), (Idle, Idle, Unknown, Idle));
        assert!(marks.is_empty());
    }

    #[test]
    fn marking_the_visible_tab_waits_until_it_is_viewed_again() {
        let mut marks = UnreadMarks::default();
        let visible = HashSet::from(["agent"]);
        marks.mark(["agent".to_owned()], &visible);
        assert!(
            !marks.acknowledge(&visible),
            "still on screen after marking"
        );
        assert!(
            !marks.acknowledge(&HashSet::from(["shell"])),
            "switched away"
        );
        assert!(marks.acknowledge(&visible), "came back");
    }

    #[test]
    fn leaving_the_machine_counts_as_looking_away() {
        let mut marks = UnreadMarks::default();
        let visible = HashSet::from(["agent"]);
        marks.mark(["agent".to_owned()], &visible);
        marks.leave_view();
        assert!(marks.acknowledge(&visible));
    }

    #[test]
    fn blocked_agents_keep_their_higher_priority_status() {
        let mut marks = UnreadMarks::default();
        let mut snapshot = fixture();
        snapshot.agents[0].agent_status = AgentStatus::Blocked;
        marks.mark(["agent".to_owned()], &HashSet::new());
        marks.apply(&mut snapshot);
        assert_eq!(snapshot.agents[0].agent_status, AgentStatus::Blocked);
    }

    #[test]
    fn closed_panes_drop_their_marks() {
        let mut marks = UnreadMarks::default();
        let mut snapshot = fixture();
        marks.mark(["gone".to_owned()], &HashSet::new());
        marks.apply(&mut snapshot);
        assert!(marks.is_empty());
    }
}
