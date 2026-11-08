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
    PreviousSegment,
    NextSegment,
    /// The n-th server group in the sidebar, counted from zero.
    Group(usize),
    /// Puts the panel away, as closing a window would.
    Close,
    Quit,
}

impl Shortcut {
    /// Whether the shortcut still applies while a text field has the keyboard.
    ///
    /// Command shortcuts never type anything, and ↑/↓ move through the list rather than through a
    /// one-line field, so they keep working while the search field is focused. Delete, Return, and
    /// ←/→ belong to the field: they edit what is being typed.
    #[must_use]
    pub const fn applies_while_typing(self) -> bool {
        !matches!(
            self,
            Self::Launch | Self::Delete | Self::PreviousSegment | Self::NextSegment
        )
    }
}

#[must_use]
pub fn interpret(key: &Key, modifiers: Modifiers) -> Option<Shortcut> {
    match key.as_ref() {
        Key::Character("n") if modifiers.command() && modifiers.shift() => Some(Shortcut::AddGroup),
        Key::Character("n") if modifiers.command() => Some(Shortcut::AddItem),
        Key::Character("f") if modifiers.command() => Some(Shortcut::FocusSearch),
        Key::Character("e") if modifiers.command() => Some(Shortcut::Edit),
        Key::Character(",") if modifiers.command() => Some(Shortcut::OpenSettings),
        Key::Character("w") if modifiers.command() => Some(Shortcut::Close),
        Key::Character("q") if modifiers.command() => Some(Shortcut::Quit),
        Key::Character(digit) if modifiers.command() => digit
            .parse::<usize>()
            .ok()
            .filter(|digit| (1..=9).contains(digit))
            .map(|digit| Shortcut::Group(digit - 1)),
        Key::Named(Named::Enter) if !modifiers.command() => Some(Shortcut::Launch),
        Key::Named(Named::Backspace | Named::Delete) if !modifiers.command() => {
            Some(Shortcut::Delete)
        }
        Key::Named(Named::ArrowUp) if !modifiers.command() => Some(Shortcut::Previous),
        Key::Named(Named::ArrowDown) if !modifiers.command() => Some(Shortcut::Next),
        Key::Named(Named::ArrowLeft) if !modifiers.command() => Some(Shortcut::PreviousSegment),
        Key::Named(Named::ArrowRight) if !modifiers.command() => Some(Shortcut::NextSegment),
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
        Shortcut::PreviousSegment => "←",
        Shortcut::NextSegment => "→",
        Shortcut::Group(_) => "⌘1–⌘9",
        Shortcut::Close => "⌘W",
        Shortcut::Quit => "⌘Q",
    }
}

#[cfg(test)]
mod tests {
    use iced::keyboard::{Key, Modifiers, key::Named};

    use super::{Shortcut, equivalent, interpret};

    #[test]
    fn common_macos_shortcuts_map_to_application_actions() {
        let command =
            |character: &str| interpret(&Key::Character(character.into()), Modifiers::COMMAND);

        assert_eq!(command("n"), Some(Shortcut::AddItem));
        assert_eq!(
            interpret(
                &Key::Character("n".into()),
                Modifiers::COMMAND | Modifiers::SHIFT
            ),
            Some(Shortcut::AddGroup)
        );
        assert_eq!(command("f"), Some(Shortcut::FocusSearch));
        assert_eq!(command("e"), Some(Shortcut::Edit));
        assert_eq!(command(","), Some(Shortcut::OpenSettings));
        assert_eq!(command("w"), Some(Shortcut::Close));
        assert_eq!(command("q"), Some(Shortcut::Quit));
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
    fn command_digits_jump_to_the_matching_group() {
        let command =
            |character: &str| interpret(&Key::Character(character.into()), Modifiers::COMMAND);

        assert_eq!(command("1"), Some(Shortcut::Group(0)));
        assert_eq!(command("9"), Some(Shortcut::Group(8)));
        assert_eq!(command("0"), None);
    }

    #[test]
    fn arrows_walk_the_list_and_the_segments() {
        let arrow = |named| interpret(&Key::Named(named), Modifiers::empty());

        assert_eq!(arrow(Named::ArrowUp), Some(Shortcut::Previous));
        assert_eq!(arrow(Named::ArrowDown), Some(Shortcut::Next));
        assert_eq!(arrow(Named::ArrowLeft), Some(Shortcut::PreviousSegment));
        assert_eq!(arrow(Named::ArrowRight), Some(Shortcut::NextSegment));
    }

    #[test]
    fn keys_that_edit_text_stay_with_a_focused_field() {
        assert!(!Shortcut::Delete.applies_while_typing());
        assert!(!Shortcut::Launch.applies_while_typing());
        assert!(!Shortcut::PreviousSegment.applies_while_typing());
        assert!(Shortcut::Next.applies_while_typing());
        assert!(Shortcut::Quit.applies_while_typing());
    }

    #[test]
    fn typing_plain_text_never_triggers_a_command() {
        for character in ["n", "f", "e", ",", "q", "w", "1"] {
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
            Shortcut::PreviousSegment,
            Shortcut::NextSegment,
            Shortcut::Group(0),
            Shortcut::Close,
            Shortcut::Quit,
        ] {
            assert_ne!(equivalent(shortcut), "");
        }
    }
}
