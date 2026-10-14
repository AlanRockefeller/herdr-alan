//! Panes and tabs the user marked unread, like marking an email unread.
//!
//! This is TUI presentation state kept per endpoint. A marked pane shows the
//! `done` status on its agent row, tab, and workspace until the user views it
//! again. Marking a tab marks the tab as a whole, so plain shells show it too
//! and viewing the tab clears it even while a hidden pane is zoomed out of view.

use std::collections::{HashMap, HashSet};

use crate::api::schema::AgentStatus;
use crate::protocol::{ClientShellAgent, ClientShellSnapshot, PaneSurfaceFrame};

#[derive(Clone, Debug, Default)]
pub(super) struct UnreadMarks {
    /// Marked pane IDs. `true` while the pane was visible when marked: it is
    /// only cleared after the user looks away and then views it again.
    panes: HashMap<String, bool>,
    /// Marked tab IDs, with the same waiting flag as `panes`.
    tabs: HashMap<String, bool>,
    /// Tabs the user marked, for the tab bar's dot, refreshed by `apply`.
    /// Tabs holding a marked agent are left out: the user marked the agent.
    marked_tabs: HashSet<String>,
    /// Statuses replaced by `apply`, restored before the next projection.
    originals: Originals,
}

#[derive(Clone, Debug, Default)]
struct Originals {
    agents: HashMap<String, AgentStatus>,
    tabs: HashMap<String, AgentStatus>,
    workspaces: HashMap<String, AgentStatus>,
}

/// What the user sees on the active endpoint's current pane surface.
#[derive(Debug, Default)]
pub(super) struct Visible<'a> {
    panes: HashSet<&'a str>,
    tabs: HashSet<&'a str>,
}

impl<'a> Visible<'a> {
    /// The panes on `surface` and their tabs, or `None` when `surface` was not
    /// projected from `snapshot` and so may not have been shown yet.
    pub(super) fn presented(
        snapshot: &'a ClientShellSnapshot,
        surface: &'a PaneSurfaceFrame,
    ) -> Option<Self> {
        if snapshot.boot_id != surface.boot_id || snapshot.revision != surface.projection_revision {
            return None;
        }
        Some(Self::on_surface(snapshot, surface))
    }

    /// The panes on `surface` and their tabs, without checking that it is current.
    pub(super) fn on_surface(
        snapshot: &'a ClientShellSnapshot,
        surface: &'a PaneSurfaceFrame,
    ) -> Self {
        let panes: HashSet<&str> = surface
            .panes
            .iter()
            .map(|pane| pane.pane_id.as_str())
            .collect();
        let tabs = snapshot
            .panes
            .iter()
            .filter(|pane| panes.contains(pane.pane_id.as_str()))
            .map(|pane| pane.tab_id.as_str())
            .collect();
        Self { panes, tabs }
    }
}

impl UnreadMarks {
    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.panes.is_empty() && self.tabs.is_empty()
    }

    /// Tabs the user marked from the tab menu, as of the last `apply`.
    pub(super) fn marked_tabs(&self) -> &HashSet<String> {
        &self.marked_tabs
    }

    pub(super) fn mark_panes(
        &mut self,
        pane_ids: impl IntoIterator<Item = String>,
        visible: &Visible<'_>,
    ) {
        for pane_id in pane_ids {
            let waiting = visible.panes.contains(pane_id.as_str());
            self.panes.insert(pane_id, waiting);
        }
    }

    pub(super) fn mark_tab(&mut self, tab_id: String, visible: &Visible<'_>) {
        let waiting = visible.tabs.contains(tab_id.as_str());
        self.tabs.insert(tab_id, waiting);
    }

    /// The user is no longer looking at this endpoint's panes.
    pub(super) fn leave_view(&mut self) {
        for waiting in self.panes.values_mut().chain(self.tabs.values_mut()) {
            *waiting = false;
        }
    }

    /// Clears marks on visible panes and tabs the user has come back to.
    /// Returns whether any cleared.
    pub(super) fn acknowledge(&mut self, visible: &Visible<'_>) -> bool {
        let before = self.panes.len() + self.tabs.len();
        acknowledge(&mut self.panes, &visible.panes);
        acknowledge(&mut self.tabs, &visible.tabs);
        self.panes.len() + self.tabs.len() != before
    }

    /// The status `agent` had before a mark raised it, for checks that must
    /// not mistake a manual mark for the agent actually finishing.
    pub(super) fn unmarked_status(&self, agent: &ClientShellAgent) -> AgentStatus {
        self.originals
            .agents
            .get(&agent.pane_id)
            .copied()
            .unwrap_or(agent.agent_status)
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

    /// Shows marked panes and tabs, and their workspaces, as `done`.
    pub(super) fn apply(&mut self, snapshot: &mut ClientShellSnapshot) {
        let pane_ids: HashSet<&str> = snapshot
            .panes
            .iter()
            .map(|pane| pane.pane_id.as_str())
            .collect();
        let tab_ids: HashSet<&str> = snapshot
            .tabs
            .iter()
            .map(|tab| tab.tab_id.as_str())
            .collect();
        self.panes
            .retain(|pane_id, _| pane_ids.contains(pane_id.as_str()));
        self.tabs
            .retain(|tab_id, _| tab_ids.contains(tab_id.as_str()));
        self.marked_tabs = self.tabs.keys().cloned().collect();
        let mut tabs = self.marked_tabs.clone();
        for pane in &snapshot.panes {
            if self.panes.contains_key(&pane.pane_id) {
                tabs.insert(pane.tab_id.clone());
            }
        }
        if tabs.is_empty() {
            return;
        }
        let workspaces: HashSet<String> = snapshot
            .tabs
            .iter()
            .filter(|tab| tabs.contains(&tab.tab_id))
            .map(|tab| tab.workspace_id.clone())
            .collect();
        for agent in &mut snapshot.agents {
            if self.panes.contains_key(&agent.pane_id) || self.tabs.contains_key(&agent.tab_id) {
                raise(
                    &mut agent.agent_status,
                    &agent.pane_id,
                    &mut self.originals.agents,
                );
            }
        }
        for tab in &mut snapshot.tabs {
            if tabs.contains(&tab.tab_id) {
                raise(&mut tab.agent_status, &tab.tab_id, &mut self.originals.tabs);
            }
        }
        for workspace in &mut snapshot.workspaces {
            if workspaces.contains(&workspace.workspace_id) {
                raise(
                    &mut workspace.agent_status,
                    &workspace.workspace_id,
                    &mut self.originals.workspaces,
                );
            }
        }
    }
}

/// Clears marks in `marks` that are visible again after the user looked away.
fn acknowledge(marks: &mut HashMap<String, bool>, visible: &HashSet<&str>) {
    marks.retain(|id, waiting| {
        if visible.contains(id.as_str()) {
            *waiting
        } else {
            *waiting = false;
            true
        }
    });
}

/// Shows `done` unless the current status needs more attention.
fn raise(status: &mut AgentStatus, id: &str, originals: &mut HashMap<String, AgentStatus>) {
    if super::status_priority(*status) < super::status_priority(AgentStatus::Done) {
        originals.entry(id.to_owned()).or_insert(*status);
        *status = AgentStatus::Done;
    }
}

impl super::ClientShellState {
    /// Marks a tab on the active endpoint unread as a whole.
    pub(super) fn mark_tab_unread(&mut self, tab_id: &str) {
        let endpoint_id = self.active_endpoint_id.clone();
        self.mark_unread(&endpoint_id, |marks, visible| {
            marks.mark_tab(tab_id.to_owned(), visible);
        });
    }

    pub(super) fn mark_panes_unread(
        &mut self,
        endpoint_id: &crate::client::endpoint::ClientEndpointId,
        pane_ids: Vec<String>,
    ) {
        self.mark_unread(endpoint_id, |marks, visible| {
            marks.mark_panes(pane_ids, visible);
        });
    }

    fn mark_unread(
        &mut self,
        endpoint_id: &crate::client::endpoint::ClientEndpointId,
        mark: impl FnOnce(&mut UnreadMarks, &Visible<'_>),
    ) {
        let active = endpoint_id == &self.active_endpoint_id;
        let visible = match (active, self.snapshot.as_deref(), self.pane_surface.as_ref()) {
            (true, Some(snapshot), Some(surface)) => Visible::on_surface(snapshot, surface),
            _ => Visible::default(),
        };
        let Some(endpoint) = self
            .endpoints
            .iter_mut()
            .find(|endpoint| &endpoint.endpoint_id == endpoint_id)
        else {
            return;
        };
        mark(&mut endpoint.unread, &visible);
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
    use crate::protocol::ClientShellPane;

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

    /// Tab `t1` holds the agent; tab `t2` holds a plain shell and a second shell
    /// hidden behind it while the tab is zoomed.
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
        snapshot.panes = vec![
            pane("agent", "t1"),
            pane("shell", "t2"),
            pane("hidden", "t2"),
        ];
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

    fn showing<'a>(pane_id: &'a str, tab_id: &'a str) -> Visible<'a> {
        Visible {
            panes: HashSet::from([pane_id]),
            tabs: HashSet::from([tab_id]),
        }
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
        marks.mark_panes(["agent".to_owned()], &Visible::default());
        marks.apply(&mut snapshot);
        use AgentStatus::*;
        assert_eq!(statuses(&snapshot), (Done, Done, Unknown, Done));
        assert!(
            marks.marked_tabs().is_empty(),
            "the tab bar dot is only for tabs the user marked"
        );
    }

    #[test]
    fn marked_tab_shows_done_on_its_agents_tab_and_workspace() {
        let mut marks = UnreadMarks::default();
        let mut snapshot = fixture();
        marks.mark_tab("t1".to_owned(), &Visible::default());
        marks.apply(&mut snapshot);
        use AgentStatus::*;
        assert_eq!(statuses(&snapshot), (Done, Done, Unknown, Done));
    }

    #[test]
    fn marked_plain_shell_tab_shows_done() {
        let mut marks = UnreadMarks::default();
        let mut snapshot = fixture();
        marks.mark_tab("t2".to_owned(), &Visible::default());
        marks.apply(&mut snapshot);
        use AgentStatus::*;
        assert_eq!(statuses(&snapshot), (Idle, Idle, Done, Done));
        assert_eq!(marks.marked_tabs(), &HashSet::from(["t2".to_owned()]));
    }

    #[test]
    fn viewing_a_marked_tab_clears_it_and_restores_statuses() {
        let mut marks = UnreadMarks::default();
        let mut snapshot = fixture();
        marks.mark_tab("t2".to_owned(), &Visible::default());
        marks.apply(&mut snapshot);
        marks.restore(&mut snapshot);
        assert!(marks.acknowledge(&showing("shell", "t2")));
        marks.apply(&mut snapshot);
        use AgentStatus::*;
        assert_eq!(statuses(&snapshot), (Idle, Idle, Unknown, Idle));
        assert!(marks.is_empty());
        assert!(marks.marked_tabs().is_empty());
    }

    #[test]
    fn viewing_a_zoomed_tab_clears_its_tab_mark() {
        let mut marks = UnreadMarks::default();
        marks.mark_tab("t2".to_owned(), &Visible::default());
        // Only the zoomed pane is on screen; the hidden pane is not.
        assert!(marks.acknowledge(&showing("shell", "t2")));
        assert!(marks.is_empty());
    }

    #[test]
    fn a_marked_pane_hidden_by_zoom_stays_marked() {
        let mut marks = UnreadMarks::default();
        marks.mark_panes(["hidden".to_owned()], &Visible::default());
        assert!(!marks.acknowledge(&showing("shell", "t2")));
        assert!(!marks.is_empty());
    }

    #[test]
    fn marking_the_visible_tab_waits_until_it_is_viewed_again() {
        let mut marks = UnreadMarks::default();
        let visible = showing("agent", "t1");
        marks.mark_tab("t1".to_owned(), &visible);
        assert!(
            !marks.acknowledge(&visible),
            "still on screen after marking"
        );
        assert!(!marks.acknowledge(&showing("shell", "t2")), "switched away");
        assert!(marks.acknowledge(&visible), "came back");
    }

    #[test]
    fn leaving_the_machine_counts_as_looking_away() {
        let mut marks = UnreadMarks::default();
        let visible = showing("agent", "t1");
        marks.mark_panes(["agent".to_owned()], &visible);
        marks.leave_view();
        assert!(marks.acknowledge(&visible));
    }

    #[test]
    fn unmarked_status_reports_what_the_mark_replaced() {
        let mut marks = UnreadMarks::default();
        let mut snapshot = fixture();
        snapshot.agents[0].agent_status = AgentStatus::Working;
        marks.mark_panes(["agent".to_owned()], &Visible::default());
        marks.apply(&mut snapshot);
        assert_eq!(snapshot.agents[0].agent_status, AgentStatus::Done);
        assert_eq!(
            marks.unmarked_status(&snapshot.agents[0]),
            AgentStatus::Working
        );
    }

    #[test]
    fn blocked_agents_keep_their_higher_priority_status() {
        let mut marks = UnreadMarks::default();
        let mut snapshot = fixture();
        snapshot.agents[0].agent_status = AgentStatus::Blocked;
        marks.mark_panes(["agent".to_owned()], &Visible::default());
        marks.apply(&mut snapshot);
        assert_eq!(snapshot.agents[0].agent_status, AgentStatus::Blocked);
    }

    #[test]
    fn closed_panes_and_tabs_drop_their_marks() {
        let mut marks = UnreadMarks::default();
        let mut snapshot = fixture();
        marks.mark_panes(["gone".to_owned()], &Visible::default());
        marks.mark_tab("gone".to_owned(), &Visible::default());
        marks.apply(&mut snapshot);
        assert!(marks.is_empty());
    }

    #[test]
    fn surfaces_from_another_projection_are_not_presented() {
        let snapshot = fixture();
        let mut surface = crate::client::shell::tests::surface();
        surface.boot_id = snapshot.boot_id.clone();
        surface.projection_revision = snapshot.revision;
        assert!(Visible::presented(&snapshot, &surface).is_some());
        surface.projection_revision += 1;
        assert!(Visible::presented(&snapshot, &surface).is_none());
    }
}
