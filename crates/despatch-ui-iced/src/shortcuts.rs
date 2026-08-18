use iced::keyboard::{Key, Modifiers, key::Named};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Shortcut {
    AddItem,
    AddGroup,
    FocusSearch,
    OpenSettings,
    Launch,
    Edit,
    Delete,
    Previous,
    Next,
    PreviousGroup,
    NextGroup,
}

#[must_use]
pub fn interpret(key: &Key, modifiers: Modifiers) -> Option<Shortcut> {
    match key.as_ref() {
        Key::Character("n") if modifiers.command() && modifiers.shift() => Some(Shortcut::AddGroup),
        Key::Character("n") if modifiers.command() => Some(Shortcut::AddItem),
        Key::Character("f") if modifiers.command() => Some(Shortcut::FocusSearch),
        Key::Character("e") if modifiers.command() => Some(Shortcut::Edit),
        Key::Character(",") if modifiers.command() => Some(Shortcut::OpenSettings),
        Key::Named(Named::Enter) if !modifiers.command() => Some(Shortcut::Launch),
        Key::Named(Named::Backspace | Named::Delete) if !modifiers.command() => {
            Some(Shortcut::Delete)
        }
        Key::Named(Named::ArrowUp) if !modifiers.command() => Some(Shortcut::Previous),
        Key::Named(Named::ArrowDown) if !modifiers.command() => Some(Shortcut::Next),
        Key::Named(Named::ArrowLeft) if !modifiers.command() => Some(Shortcut::PreviousGroup),
        Key::Named(Named::ArrowRight) if !modifiers.command() => Some(Shortcut::NextGroup),
        _ => None,
    }
}

/// The keyboard equivalent shown next to a menu-style control.
#[must_use]
pub const fn equivalent(shortcut: Shortcut) -> &'static str {
    match shortcut {
        Shortcut::AddItem => "⌘N",
        Shortcut::AddGroup => "⇧⌘N",
        Shortcut::FocusSearch => "⌘F",
        Shortcut::OpenSettings => "⌘,",
        Shortcut::Launch => "Return",
        Shortcut::Edit => "⌘E",
        Shortcut::Delete => "Delete",
        Shortcut::Previous => "↑",
        Shortcut::Next => "↓",
        Shortcut::PreviousGroup => "←",
        Shortcut::NextGroup => "→",
    }
}

#[cfg(test)]
mod tests {
    use iced::keyboard::{Key, Modifiers, key::Named};

    use super::{Shortcut, equivalent, interpret};

    #[test]
    fn common_macos_shortcuts_map_to_application_actions() {
        assert_eq!(
            interpret(&Key::Character("n".into()), Modifiers::COMMAND),
            Some(Shortcut::AddItem)
        );
        assert_eq!(
            interpret(
                &Key::Character("n".into()),
                Modifiers::COMMAND | Modifiers::SHIFT
            ),
            Some(Shortcut::AddGroup)
        );
        assert_eq!(
            interpret(&Key::Character("f".into()), Modifiers::COMMAND),
            Some(Shortcut::FocusSearch)
        );
        assert_eq!(
            interpret(&Key::Character("e".into()), Modifiers::COMMAND),
            Some(Shortcut::Edit)
        );
        assert_eq!(
            interpret(&Key::Named(Named::Enter), Modifiers::empty()),
            Some(Shortcut::Launch)
        );
        assert_eq!(
            interpret(&Key::Named(Named::Backspace), Modifiers::empty()),
            Some(Shortcut::Delete)
        );
    }

    #[test]
    fn typing_plain_text_never_triggers_a_command() {
        for character in ["n", "f", "e", ","] {
            assert_eq!(
                interpret(&Key::Character(character.into()), Modifiers::empty()),
                None
            );
        }
    }

    #[test]
    fn every_shortcut_has_a_keyboard_equivalent_to_show() {
        for shortcut in [
            Shortcut::AddItem,
            Shortcut::AddGroup,
            Shortcut::FocusSearch,
            Shortcut::OpenSettings,
            Shortcut::Launch,
            Shortcut::Edit,
            Shortcut::Delete,
            Shortcut::Previous,
            Shortcut::Next,
            Shortcut::PreviousGroup,
            Shortcut::NextGroup,
        ] {
            assert!(!equivalent(shortcut).is_empty());
        }
    }
}
