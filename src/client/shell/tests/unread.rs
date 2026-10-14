use super::*;
use crate::client::endpoint::ClientEndpointId;

/// Two tabs: `tab_1` (visible, `pane_1`) and `tab_2` with a plain shell `pane_2`.
fn two_tab_state() -> ClientShellState {
    let mut projected = snapshot();
    let mut tab = projected.tabs[0].clone();
    tab.tab_id = "tab_2".into();
    tab.number = 2;
    tab.focused = false;
    tab.agent_status = AgentStatus::Unknown;
    projected.tabs.push(tab);
    let mut pane = projected.panes[0].clone();
    pane.pane_id = "pane_2".into();
    pane.tab_id = "tab_2".into();
    pane.focused = false;
    projected.panes.push(pane);
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(projected));
    state.set_pane_surface(surface());
    state
}

fn tab_status(state: &ClientShellState, tab_id: &str) -> AgentStatus {
    state
        .snapshot
        .as_deref()
        .and_then(|snapshot| snapshot.tabs.iter().find(|tab| tab.tab_id == tab_id))
        .map(|tab| tab.agent_status)
        .unwrap()
}

fn surface_showing(pane_id: &str) -> PaneSurfaceFrame {
    let mut frame = surface();
    for pane in &mut frame.panes {
        pane.pane_id = pane_id.into();
    }
    frame
}

fn mark_tab_from_menu(state: &mut ClientShellState, tab_id: &str) -> ClientShellInput {
    state.open_tab_context_menu(tab_id.into(), 30, 1);
    let Some(ClientShellOverlay::ContextMenu(menu)) = state.overlay.as_ref() else {
        panic!("tab menu did not open");
    };
    let index = menu
        .items()
        .iter()
        .position(|item| item.action == ClientContextMenuAction::MarkUnread)
        .expect("tab menu offers Mark unread");
    let mut outcome = ClientShellInput::default();
    state.activate_context_menu_item(index, &mut outcome);
    outcome
}

#[test]
fn marking_a_background_shell_tab_shows_done_without_switching_to_it() {
    let mut state = two_tab_state();
    let outcome = mark_tab_from_menu(&mut state, "tab_2");
    assert!(
        outcome.actions.is_empty(),
        "marking must not focus the tab: {:?}",
        outcome.actions
    );
    assert_eq!(tab_status(&state, "tab_2"), AgentStatus::Done);
    assert_eq!(tab_status(&state, "tab_1"), AgentStatus::Idle);

    // Viewing the tab clears the mark and restores its original status.
    assert!(state.acknowledge_active_surface_agents(&surface_showing("pane_2")));
    assert_eq!(tab_status(&state, "tab_2"), AgentStatus::Unknown);
}

#[test]
fn marking_the_visible_tab_lasts_until_it_is_viewed_again() {
    let mut state = two_tab_state();
    mark_tab_from_menu(&mut state, "tab_1");
    assert_eq!(tab_status(&state, "tab_1"), AgentStatus::Done);

    state.acknowledge_active_surface_agents(&surface_showing("pane_1"));
    assert_eq!(
        tab_status(&state, "tab_1"),
        AgentStatus::Done,
        "still being looked at"
    );
    state.acknowledge_active_surface_agents(&surface_showing("pane_2"));
    assert_eq!(
        tab_status(&state, "tab_1"),
        AgentStatus::Done,
        "switched away"
    );
    state.acknowledge_active_surface_agents(&surface_showing("pane_1"));
    assert_eq!(tab_status(&state, "tab_1"), AgentStatus::Idle, "came back");
}

#[test]
fn marks_survive_new_server_snapshots_until_viewed() {
    let mut state = two_tab_state();
    mark_tab_from_menu(&mut state, "tab_2");
    let mut next = state.snapshot.as_deref().unwrap().clone();
    next.revision += 1;
    // The server's own statuses come back unprojected.
    next.tabs[1].agent_status = AgentStatus::Unknown;
    state.set_snapshot(Box::new(next));
    assert_eq!(tab_status(&state, "tab_2"), AgentStatus::Done);
}

#[test]
fn agent_row_menu_marks_that_agent_unread() {
    let mut state = two_tab_state();
    state.open_agent_context_menu(ClientEndpointId::Local, "pane_2".into(), 2, 10);
    let Some(ClientShellOverlay::ContextMenu(menu)) = state.overlay.as_ref() else {
        panic!("agent menu did not open");
    };
    assert_eq!(menu.items().len(), 1);
    assert_eq!(menu.items()[0].label, "Mark unread");
    let mut outcome = ClientShellInput::default();
    state.activate_context_menu_item(0, &mut outcome);
    assert_eq!(tab_status(&state, "tab_2"), AgentStatus::Done);
    assert!(
        marked_tabs(&state).is_empty(),
        "marking an agent does not dot its tab"
    );
}

fn marked_tabs(state: &ClientShellState) -> Vec<String> {
    let mut tabs: Vec<String> = state.endpoints[0]
        .unread
        .marked_tabs()
        .iter()
        .cloned()
        .collect();
    tabs.sort();
    tabs
}

#[test]
fn only_marked_tabs_get_the_tab_bar_dot() {
    let mut state = two_tab_state();
    assert!(marked_tabs(&state).is_empty());
    mark_tab_from_menu(&mut state, "tab_2");
    assert_eq!(marked_tabs(&state), ["tab_2"]);
    state.acknowledge_active_surface_agents(&surface_showing("pane_2"));
    assert!(marked_tabs(&state).is_empty());
}

#[test]
fn a_surface_ahead_of_the_snapshot_does_not_clear_marks() {
    let mut state = two_tab_state();
    mark_tab_from_menu(&mut state, "tab_2");
    let mut ahead = surface_showing("pane_2");
    ahead.projection_revision += 1;
    assert!(!state.acknowledge_active_surface_agents(&ahead));
    assert_eq!(tab_status(&state, "tab_2"), AgentStatus::Done);
}

#[test]
fn marking_a_working_agent_does_not_validate_a_finished_notification() {
    use crate::protocol::{SemanticNotification, SemanticNotificationKind};
    let mut config = ClientShellConfig::from_config(&Config::default());
    config.toast_delivery = crate::config::ToastDelivery::Terminal;
    config.toast_delay_seconds = 0;
    let mut state = ClientShellState::new(config);
    let mut projected = snapshot();
    projected.agents.push(crate::protocol::ClientShellAgent {
        pane_id: "pane_1".into(),
        workspace_id: "ws_1".into(),
        tab_id: "tab_1".into(),
        name: Some("agent".into()),
        display_agent: None,
        agent: Some("agent".into()),
        title: None,
        terminal_title: None,
        terminal_title_stripped: None,
        agent_status: AgentStatus::Working,
        state_change_seq: 1,
        state_labels: Vec::new(),
        tokens: Vec::new(),
        focused: false,
    });
    state.set_snapshot(Box::new(projected));
    state.mark_panes_unread(&ClientEndpointId::Local, vec!["pane_1".into()]);
    assert_eq!(
        state.snapshot.as_deref().unwrap().agents[0].agent_status,
        AgentStatus::Done
    );

    let (effects, _) = state.receive_notification(
        &ClientEndpointId::Local,
        SemanticNotification {
            kind: SemanticNotificationKind::Finished,
            title: "agent finished".into(),
            body: None,
            sound: None,
            agent: Some("agent".into()),
            workspace_id: Some("ws_1".into()),
            tab_id: Some("background-tab".into()),
            pane_id: Some("pane_1".into()),
            position: None,
        },
        std::time::Instant::now(),
    );
    assert!(effects.is_empty(), "still working, so the finish must wait");
}
