//! User intent, decoupled from key bindings. A single place maps a key press to an
//! [`Action`]; the rest of the TUI reasons in `Action`s, which leaves room for a
//! configurable keymap later without touching the app logic.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// A semantic UI intent produced from a key press.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Quit,
    Up,
    Down,
    /// Drill in / focus the detail pane.
    Right,
    /// Step out / focus the list pane.
    Left,
    Enter,
    Back,
    Tab,
    Top,
    Bottom,
    PageUp,
    PageDown,
    ToggleDiff,
    ToggleTranscript,
    Refresh,
    Help,
}

/// Map a key press to an [`Action`], or `None` if the key is unbound. Key *release*
/// events (some terminals emit them) are ignored so actions don't fire twice.
pub fn action_from_key(key: KeyEvent) -> Option<Action> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let action = match key.code {
        KeyCode::Char('c') if ctrl => Action::Quit,
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Up | KeyCode::Char('k') => Action::Up,
        KeyCode::Down | KeyCode::Char('j') => Action::Down,
        KeyCode::Right | KeyCode::Char('l') => Action::Right,
        KeyCode::Left | KeyCode::Char('h') => Action::Left,
        KeyCode::Enter => Action::Enter,
        KeyCode::Esc | KeyCode::Backspace => Action::Back,
        KeyCode::Tab => Action::Tab,
        KeyCode::Char('g') => Action::Top,
        KeyCode::Char('G') => Action::Bottom,
        KeyCode::PageUp => Action::PageUp,
        KeyCode::PageDown => Action::PageDown,
        KeyCode::Char('d') => Action::ToggleDiff,
        KeyCode::Char('t') => Action::ToggleTranscript,
        KeyCode::Char('r') => Action::Refresh,
        KeyCode::Char('?') => Action::Help,
        _ => return None,
    };
    Some(action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn maps_keys_to_actions() {
        assert_eq!(action_from_key(key(KeyCode::Char('j'))), Some(Action::Down));
        assert_eq!(action_from_key(key(KeyCode::Down)), Some(Action::Down));
        assert_eq!(action_from_key(key(KeyCode::Char('k'))), Some(Action::Up));
        assert_eq!(action_from_key(key(KeyCode::Char('l'))), Some(Action::Right));
        assert_eq!(action_from_key(key(KeyCode::Char('d'))), Some(Action::ToggleDiff));
        assert_eq!(action_from_key(key(KeyCode::Char('t'))), Some(Action::ToggleTranscript));
        assert_eq!(action_from_key(key(KeyCode::Char('q'))), Some(Action::Quit));
        assert_eq!(action_from_key(key(KeyCode::Esc)), Some(Action::Back));
        assert_eq!(action_from_key(key(KeyCode::Char('z'))), None);
    }

    #[test]
    fn ctrl_c_quits_and_release_is_ignored() {
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(action_from_key(ctrl_c), Some(Action::Quit));

        let mut release = key(KeyCode::Char('j'));
        release.kind = KeyEventKind::Release;
        assert_eq!(action_from_key(release), None);
    }
}
