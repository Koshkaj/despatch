use despatch_application::Reachability;
use despatch_domain::{
    ColorViolation, GroupId, LabelColor, Launcher, LauncherId, PathTemplate, SshLauncher, SshPort,
    SubgroupId,
};
use iced::widget::text_editor;

/// The editor field an inline validation message belongs to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Field {
    Name,
    Color,
    Host,
    User,
    Port,
    IdentityFile,
    Args,
}

/// A validation failure, attached to the field that caused it whenever one can be identified.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormError {
    pub field: Option<Field>,
    pub message: String,
}

impl FormError {
    pub(crate) fn field(field: Field, message: impl Into<String>) -> Self {
        Self {
            field: Some(field),
            message: message.into(),
        }
    }

    fn general(message: impl Into<String>) -> Self {
        Self {
            field: None,
            message: message.into(),
        }
    }
}

/// Whether an editor creates a new entity or replaces an existing one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target<Id> {
    New,
    Existing(Id),
}

/// What a [`NameDraft`] is naming.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NameTarget {
    NewGroup,
    RenameGroup(GroupId),
    /// A new category inside the given group.
    NewSubgroup(GroupId),
    RenameSubgroup(SubgroupId),
}

/// The validated contents of a [`NameDraft`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NameValues {
    pub name: String,
    pub color: Option<LabelColor>,
}

/// The editor behind creating and renaming both groups and categories.
///
/// Only a group carries a color. A category is always drawn in the color of the group holding it,
/// so offering it one here would be offering a setting that could not take effect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NameDraft {
    pub target: NameTarget,
    pub name: String,
    /// The typed color, kept as text so a half-typed hex value is never discarded.
    pub color: String,
    pub error: Option<FormError>,
}

impl NameDraft {
    #[must_use]
    pub const fn new(target: NameTarget) -> Self {
        Self {
            target,
            name: String::new(),
            color: String::new(),
            error: None,
        }
    }

    #[must_use]
    pub fn rename(target: NameTarget, name: impl Into<String>, color: Option<LabelColor>) -> Self {
        Self {
            target,
            name: name.into(),
            color: color.map(LabelColor::to_hex).unwrap_or_default(),
            error: None,
        }
    }

    pub fn set_name(&mut self, name: String) {
        self.name = name;
        self.clear_error(Field::Name);
    }

    pub fn set_color(&mut self, color: String) {
        self.color = color;
        self.clear_error(Field::Color);
    }

    /// Selects a swatch, or clears the color when the selected swatch is pressed again.
    pub fn toggle_color(&mut self, color: LabelColor) {
        self.color = if self.selected_color() == Some(color) {
            String::new()
        } else {
            color.to_hex()
        };
        self.clear_error(Field::Color);
    }

    /// The color the swatches should show as selected, ignoring text that does not parse yet.
    #[must_use]
    pub fn selected_color(&self) -> Option<LabelColor> {
        LabelColor::parse(&self.color).ok()
    }

    /// Whether this draft names something that can carry a color of its own.
    #[must_use]
    pub const fn has_color(&self) -> bool {
        match self.target {
            NameTarget::NewGroup | NameTarget::RenameGroup(_) => true,
            NameTarget::NewSubgroup(_) | NameTarget::RenameSubgroup(_) => false,
        }
    }

    fn clear_error(&mut self, field: Field) {
        if self
            .error
            .as_ref()
            .is_some_and(|error| error.field == Some(field))
        {
            self.error = None;
        }
    }

    /// Returns the trimmed name and its color, or the inline error to show instead.
    pub fn build(&self) -> Result<NameValues, FormError> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(FormError::field(Field::Name, self.blank_message()));
        }
        Ok(NameValues {
            name: name.to_owned(),
            color: if self.has_color() {
                parse_color(&self.color)?
            } else {
                None
            },
        })
    }

    const fn blank_message(&self) -> &'static str {
        match self.target {
            NameTarget::NewGroup | NameTarget::RenameGroup(_) => "Enter a group name.",
            NameTarget::NewSubgroup(_) | NameTarget::RenameSubgroup(_) => "Enter a category name.",
        }
    }

    #[must_use]
    pub const fn title(&self) -> &'static str {
        match self.target {
            NameTarget::NewGroup => "New server group",
            NameTarget::RenameGroup(_) => "Rename group",
            NameTarget::NewSubgroup(_) => "New category",
            NameTarget::RenameSubgroup(_) => "Rename category",
        }
    }

    #[must_use]
    pub const fn detail(&self) -> &'static str {
        match self.target {
            NameTarget::NewGroup | NameTarget::RenameGroup(_) => {
                "Groups sit at the top of a section, such as Production or Staging."
            }
            NameTarget::NewSubgroup(_) | NameTarget::RenameSubgroup(_) => {
                "Categories split a group's servers into sets."
            }
        }
    }

    #[must_use]
    pub const fn placeholder(&self) -> &'static str {
        match self.target {
            NameTarget::NewGroup | NameTarget::RenameGroup(_) => "Production",
            NameTarget::NewSubgroup(_) | NameTarget::RenameSubgroup(_) => "Server",
        }
    }

    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self.target {
            NameTarget::NewGroup | NameTarget::RenameGroup(_) => "Group name",
            NameTarget::NewSubgroup(_) | NameTarget::RenameSubgroup(_) => "Category name",
        }
    }

    #[must_use]
    pub const fn action(&self) -> &'static str {
        match self.target {
            NameTarget::NewGroup => "Create group",
            NameTarget::NewSubgroup(_) => "Create category",
            NameTarget::RenameGroup(_) | NameTarget::RenameSubgroup(_) => "Rename",
        }
    }
}

/// The server editor: one draft for creating and for editing an SSH launcher, so both flows share
/// the same validation.
#[derive(Debug)]
pub struct LauncherDraft {
    /// The category the launcher is saved into. When it differs from [`Self::origin`] for an
    /// existing launcher, saving also moves it.
    pub subgroup_id: SubgroupId,
    /// The category the launcher lived in when the editor opened.
    pub origin: SubgroupId,
    /// The categories the editor offers to choose between, in config order. Empty when there is
    /// nothing to choose.
    pub categories: Vec<CategoryChoice>,
    pub target: Target<LauncherId>,
    pub name: String,
    pub host: String,
    pub user: String,
    pub port: String,
    pub identity_file: String,
    /// Extra `ssh` arguments, one per line.
    pub args: text_editor::Content,
    /// The color override, kept as text so a half-typed hex value is never discarded. Empty means
    /// the launcher follows the color of its group.
    pub color: String,
    /// Whether the additional-settings disclosure is open.
    pub advanced: bool,
    /// What the last reachability check found, for the host and port it was run against.
    pub reachability: ReachabilityState,
    pub error: Option<FormError>,
}

/// One category a launcher can be saved into.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CategoryChoice {
    pub id: SubgroupId,
    pub name: String,
}

impl std::fmt::Display for CategoryChoice {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.name)
    }
}

/// The state of the reachability check shown under the host field.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum ReachabilityState {
    #[default]
    Untested,
    /// A check is in flight against this destination.
    Checking { destination: String },
    /// A finished check, kept with the destination it ran against so editing the host or port
    /// visibly invalidates it rather than leaving a stale verdict on screen.
    Checked {
        destination: String,
        outcome: Reachability,
    },
    /// The check itself could not run.
    Failed {
        destination: String,
        message: String,
    },
}

impl LauncherDraft {
    /// An empty server draft.
    #[must_use]
    pub fn new(subgroup_id: SubgroupId) -> Self {
        Self {
            subgroup_id,
            origin: subgroup_id,
            categories: Vec::new(),
            target: Target::New,
            name: String::new(),
            host: String::new(),
            user: String::new(),
            // Blank means the default port, so a new server only asks for what it really needs.
            port: String::new(),
            identity_file: String::new(),
            args: text_editor::Content::new(),
            color: String::new(),
            advanced: false,
            reachability: ReachabilityState::Untested,
            error: None,
        }
    }

    /// Offers `categories` to choose between. One category or none is no choice, and offers nothing.
    #[must_use]
    pub fn with_categories(mut self, categories: Vec<CategoryChoice>) -> Self {
        self.categories = if categories.len() > 1 {
            categories
        } else {
            Vec::new()
        };
        self
    }

    /// Saves the launcher into `subgroup_id`, if it is one of the categories on offer.
    pub fn select_category(&mut self, subgroup_id: SubgroupId) {
        if self
            .categories
            .iter()
            .any(|choice| choice.id == subgroup_id)
        {
            self.subgroup_id = subgroup_id;
        }
    }

    /// The category currently chosen, when the editor offers a choice.
    #[must_use]
    pub fn selected_category(&self) -> Option<&CategoryChoice> {
        self.categories
            .iter()
            .find(|choice| choice.id == self.subgroup_id)
    }

    /// A draft pre-filled from an existing server, keeping its identifier. Only SSH launchers
    /// can be edited; anything else in an older config is left as it is.
    #[must_use]
    pub fn edit(subgroup_id: SubgroupId, launcher: &Launcher) -> Option<Self> {
        let Launcher::Ssh(ssh) = launcher else {
            return None;
        };
        let mut draft = Self::new(subgroup_id);
        draft.target = Target::Existing(ssh.id);
        draft.color = ssh.color.map(LabelColor::to_hex).unwrap_or_default();
        draft.name.clone_from(&ssh.name);
        draft.host.clone_from(&ssh.host);
        draft.user.clone_from(&ssh.user);
        draft.port = if ssh.port == SshPort::DEFAULT {
            String::new()
        } else {
            ssh.port.get().to_string()
        };
        draft.identity_file = ssh
            .identity_file
            .as_ref()
            .map(|path| path.as_str().to_owned())
            .unwrap_or_default();
        draft.args = text_editor::Content::with_text(&ssh.extra_args.join("\n"));
        // A launcher that already uses an additional setting opens with them in view, so editing
        // never hides a value the user set earlier.
        draft.advanced =
            !draft.port.is_empty() || !draft.identity_file.is_empty() || !ssh.extra_args.is_empty();
        Some(draft)
    }

    /// The current text of a single-line field. The multi-line argument list is not one.
    #[must_use]
    pub fn value(&self, field: Field) -> &str {
        match field {
            Field::Name => &self.name,
            Field::Host => &self.host,
            Field::User => &self.user,
            Field::Port => &self.port,
            Field::IdentityFile => &self.identity_file,
            // The arguments are not a single-line field, and the color is held as its own text.
            Field::Args | Field::Color => "",
        }
    }

    /// Updates one text field and clears any error that pointed at it.
    pub fn set(&mut self, field: Field, value: String) {
        match field {
            Field::Name => self.name = value,
            Field::Host => self.host = value,
            Field::User => self.user = value,
            Field::Port => self.port = value,
            Field::IdentityFile => self.identity_file = value,
            Field::Args | Field::Color => return,
        }
        self.clear_error(field);
    }

    /// The `host:port` a reachability check would run against, or `None` while the host is blank.
    ///
    /// The port is resolved the same way saving resolves it, so a check never tests a port the
    /// launcher would not use.
    #[must_use]
    pub fn destination(&self) -> Option<(String, SshPort)> {
        let host = self.host.trim();
        if host.is_empty() {
            return None;
        }
        let port = if self.port.trim().is_empty() {
            SshPort::DEFAULT
        } else {
            self.port
                .trim()
                .parse::<u16>()
                .ok()
                .and_then(SshPort::new)?
        };
        Some((host.to_owned(), port))
    }

    /// The reachability verdict, but only while it still describes the current host and port.
    #[must_use]
    pub fn current_reachability(&self) -> Option<&ReachabilityState> {
        let destination = self
            .destination()
            .map(|(host, port)| format!("{host}:{}", port.get()))?;
        let matches = match &self.reachability {
            ReachabilityState::Untested => return None,
            ReachabilityState::Checking {
                destination: tested,
            }
            | ReachabilityState::Checked {
                destination: tested,
                ..
            }
            | ReachabilityState::Failed {
                destination: tested,
                ..
            } => tested == &destination,
        };
        matches.then_some(&self.reachability)
    }

    /// Opens or closes the additional-settings disclosure.
    pub fn toggle_advanced(&mut self) {
        self.advanced = !self.advanced;
    }

    pub fn set_color(&mut self, color: String) {
        self.color = color;
        self.clear_error(Field::Color);
    }

    /// Selects a swatch, or drops back to the group's color when the selected swatch is pressed
    /// again.
    pub fn toggle_color(&mut self, color: LabelColor) {
        self.color = if self.selected_color() == Some(color) {
            String::new()
        } else {
            color.to_hex()
        };
        self.clear_error(Field::Color);
    }

    /// The color the swatches should show as selected. `None` means the launcher follows its group.
    #[must_use]
    pub fn selected_color(&self) -> Option<LabelColor> {
        LabelColor::parse(&self.color).ok()
    }

    pub fn edit_args(&mut self, action: text_editor::Action) {
        self.args.perform(action);
        self.clear_error(Field::Args);
    }

    /// Builds a validated launcher, preserving the identifier when editing.
    pub fn build(&self) -> Result<Launcher, FormError> {
        let launcher = self.build_ssh()?;
        launcher
            .validate()
            .map_err(|error| FormError::general(error.to_string()))?;
        Ok(launcher)
    }

    #[must_use]
    pub const fn title(&self) -> &'static str {
        match self.target {
            Target::New => "New server",
            Target::Existing(_) => "Edit server",
        }
    }

    #[must_use]
    pub const fn action(&self) -> &'static str {
        match self.target {
            Target::New => "Add",
            Target::Existing(_) => "Save",
        }
    }

    /// The inline error for `field`, if the last save attempt blamed it.
    #[must_use]
    pub fn field_error(&self, field: Field) -> Option<&str> {
        self.error
            .as_ref()
            .filter(|error| error.field == Some(field))
            .map(|error| error.message.as_str())
    }

    /// The error that could not be attributed to a single field.
    #[must_use]
    pub fn general_error(&self) -> Option<&str> {
        self.error
            .as_ref()
            .filter(|error| error.field.is_none())
            .map(|error| error.message.as_str())
    }

    fn clear_error(&mut self, field: Field) {
        if self
            .error
            .as_ref()
            .is_some_and(|error| error.field == Some(field))
        {
            self.error = None;
        }
    }

    fn build_ssh(&self) -> Result<Launcher, FormError> {
        let name = required(&self.name, Field::Name, "Enter a server name.")?;
        let host = required(&self.host, Field::Host, "Enter a hostname or IP address.")?;
        let user = required(&self.user, Field::User, "Enter the SSH username.")?;
        // An empty override keeps the standard SSH port, which is why the field is optional.
        let port = if self.port.trim().is_empty() {
            SshPort::DEFAULT
        } else {
            self.port
                .trim()
                .parse::<u16>()
                .ok()
                .and_then(SshPort::new)
                .ok_or_else(|| FormError::field(Field::Port, "Use a port from 1 to 65535."))?
        };

        let mut ssh = SshLauncher::new(name, host, user)
            .map_err(|error| FormError::field(Field::Name, error.to_string()))?;
        ssh.color = parse_color(&self.color)?;
        ssh.port = port;
        ssh.identity_file = parse_path(&self.identity_file, Field::IdentityFile, "Identity file")?;
        ssh.extra_args = self.parsed_args();
        if let Target::Existing(launcher_id) = self.target {
            ssh.id = launcher_id;
        }
        Ok(Launcher::Ssh(ssh))
    }

    /// One argument per non-empty line, kept as separate argv entries rather than one string.
    fn parsed_args(&self) -> Vec<String> {
        self.args
            .text()
            .lines()
            .map(str::trim_end)
            .filter(|argument| !argument.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }
}

/// An empty color field means "inherit": the section accent for a group, the group's color for a
/// launcher. Anything else must be a hex color.
fn parse_color(value: &str) -> Result<Option<LabelColor>, FormError> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    LabelColor::parse(value).map(Some).map_err(|violation| {
        let ColorViolation::Malformed = violation;
        FormError::field(Field::Color, format!("Color {violation}."))
    })
}

fn required(value: &str, field: Field, message: &'static str) -> Result<String, FormError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(FormError::field(field, message));
    }
    Ok(value.to_owned())
}

fn parse_path(
    value: &str,
    field: Field,
    label: &'static str,
) -> Result<Option<PathTemplate>, FormError> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    PathTemplate::new(value)
        .map(Some)
        .map_err(|violation| FormError::field(field, format!("{label} {violation}.")))
}

#[cfg(test)]
mod tests {
    use despatch_domain::{GroupId, LabelColor, Launcher, SshLauncher, SshPort, SubgroupId};
    use iced::widget::text_editor;

    use super::{Field, LauncherDraft, NameDraft, NameTarget, Target};

    fn server_draft() -> LauncherDraft {
        LauncherDraft::new(SubgroupId::new())
    }

    #[test]
    fn a_server_draft_builds_every_structured_ssh_field() {
        let mut draft = server_draft();
        draft.set(Field::Name, "API server".to_owned());
        draft.set(Field::Host, "api.example.com".to_owned());
        draft.set(Field::User, "deploy".to_owned());
        draft.set(Field::Port, "2222".to_owned());
        draft.set(Field::IdentityFile, "$HOME/.ssh/id_ed25519".to_owned());
        draft.args = text_editor::Content::with_text("-v\n\n-o ConnectTimeout=5\n");

        let Launcher::Ssh(ssh) = draft.build().expect("valid draft") else {
            panic!("expected an SSH launcher");
        };
        assert_eq!(ssh.name, "API server");
        assert_eq!(ssh.host, "api.example.com");
        assert_eq!(ssh.user, "deploy");
        assert_eq!(ssh.port.get(), 2222);
        assert_eq!(
            ssh.identity_file
                .as_ref()
                .map(despatch_domain::PathTemplate::as_str),
            Some("$HOME/.ssh/id_ed25519")
        );
        assert_eq!(ssh.extra_args, ["-v", "-o ConnectTimeout=5"]);
    }

    #[test]
    fn missing_required_server_values_are_reported_on_their_own_field() {
        let mut draft = server_draft();
        assert_eq!(draft.build().unwrap_err().field, Some(Field::Name));

        draft.set(Field::Name, "API".to_owned());
        assert_eq!(draft.build().unwrap_err().field, Some(Field::Host));

        draft.set(Field::Host, "api.example.com".to_owned());
        assert_eq!(draft.build().unwrap_err().field, Some(Field::User));

        draft.set(Field::User, "deploy".to_owned());
        draft.set(Field::Port, "0".to_owned());
        assert_eq!(draft.build().unwrap_err().field, Some(Field::Port));

        draft.set(Field::Port, "70000".to_owned());
        assert_eq!(draft.build().unwrap_err().field, Some(Field::Port));

        draft.set(Field::Port, "22".to_owned());
        assert!(draft.build().is_ok());
    }

    #[test]
    fn an_untouched_port_override_falls_back_to_the_standard_ssh_port() {
        let mut draft = server_draft();
        draft.set(Field::Name, "API".to_owned());
        draft.set(Field::Host, "api.example.com".to_owned());
        draft.set(Field::User, "deploy".to_owned());
        assert!(draft.port.is_empty(), "a new server asks for no port");

        let Launcher::Ssh(ssh) = draft.build().expect("valid draft") else {
            panic!("expected an SSH launcher");
        };
        assert_eq!(ssh.port, SshPort::DEFAULT);
    }

    #[test]
    fn additional_settings_stay_closed_until_a_launcher_actually_uses_one() {
        let draft = server_draft();
        assert!(!draft.advanced);

        let mut ssh = SshLauncher::new("API", "api.example.com", "deploy").expect("ssh");
        let plain =
            LauncherDraft::edit(SubgroupId::new(), &Launcher::Ssh(ssh.clone())).expect("a server");
        assert!(!plain.advanced, "nothing to reveal on a default connection");
        assert!(
            plain.port.is_empty(),
            "the default port is not shown as an override"
        );

        ssh.port = SshPort::new(2222).expect("valid port");
        let overridden =
            LauncherDraft::edit(SubgroupId::new(), &Launcher::Ssh(ssh)).expect("a server");
        assert!(overridden.advanced, "an override must not be hidden");
        assert_eq!(overridden.port, "2222");
    }

    #[test]
    fn an_invalid_identity_path_blames_the_identity_field() {
        let mut draft = server_draft();
        draft.set(Field::Name, "API".to_owned());
        draft.set(Field::Host, "api.example.com".to_owned());
        draft.set(Field::User, "deploy".to_owned());
        draft.set(Field::IdentityFile, "  ".to_owned());
        assert!(draft.build().is_ok(), "blank identity files are optional");

        draft.set(Field::IdentityFile, "~/keys/id\ned25519".to_owned());
        assert_eq!(
            draft.build().unwrap_err().field,
            Some(Field::IdentityFile),
            "a rejected path template must not be reported as a name problem"
        );
    }

    #[test]
    fn editing_preserves_the_launcher_identifier_and_prefills_every_field() {
        let mut ssh = SshLauncher::new("API", "api.example.com", "deploy").expect("ssh");
        ssh.extra_args = vec!["-v".to_owned()];
        let original = Launcher::Ssh(ssh);
        let subgroup_id = SubgroupId::new();

        let mut draft = LauncherDraft::edit(subgroup_id, &original).expect("a server");
        assert_eq!(draft.target, Target::Existing(original.id()));
        assert_eq!(draft.host, "api.example.com");
        assert_eq!(draft.args.text().trim_end(), "-v");

        draft.set(Field::Host, "api2.example.com".to_owned());
        let edited = draft.build().expect("valid edit");

        assert_eq!(edited.id(), original.id());
        let Launcher::Ssh(edited) = edited else {
            panic!("expected an SSH launcher");
        };
        assert_eq!(edited.host, "api2.example.com");
    }

    #[test]
    fn a_group_draft_trims_its_name_and_rejects_blank_input() {
        let mut draft = NameDraft::new(NameTarget::NewGroup);
        assert_eq!(draft.title(), "New server group");
        assert_eq!(draft.build().unwrap_err().field, Some(Field::Name));

        draft.set_name("   ".to_owned());
        assert!(draft.build().is_err());

        draft.set_name("  Production  ".to_owned());
        let values = draft.build().expect("valid name");
        assert_eq!(values.name, "Production");
        assert_eq!(values.color, None, "a group is uncolored until asked");
    }

    #[test]
    fn a_rename_draft_starts_from_the_current_name_and_color() {
        let group_id = GroupId::new();
        let color = LabelColor::from_rgb(0x4F, 0x9D, 0xFF);
        let draft = NameDraft::rename(NameTarget::RenameGroup(group_id), "Home lab", Some(color));

        assert_eq!(draft.target, NameTarget::RenameGroup(group_id));
        assert_eq!(draft.name, "Home lab");
        assert_eq!(draft.selected_color(), Some(color));
        assert_eq!(draft.title(), "Rename group");
        assert_eq!(draft.action(), "Rename");
    }

    #[test]
    fn a_swatch_selects_its_color_and_pressing_it_again_clears_it() {
        let color = LabelColor::from_rgb(0x30, 0xD1, 0x58);
        let mut draft = NameDraft::new(NameTarget::NewGroup);
        draft.set_name("Production".to_owned());

        draft.toggle_color(color);
        assert_eq!(draft.build().expect("valid draft").color, Some(color));

        draft.toggle_color(color);
        assert_eq!(draft.selected_color(), None);
        assert_eq!(draft.build().expect("valid draft").color, None);
    }

    #[test]
    fn a_typed_color_is_accepted_as_hex_and_blamed_on_the_color_field_when_it_is_not() {
        let mut draft = NameDraft::new(NameTarget::NewGroup);
        draft.set_name("Production".to_owned());

        draft.set_color("#4f9dff".to_owned());
        assert_eq!(
            draft.build().expect("valid draft").color,
            Some(LabelColor::from_rgb(0x4F, 0x9D, 0xFF))
        );

        draft.set_color("not a color".to_owned());
        let error = draft.build().unwrap_err();
        assert_eq!(error.field, Some(Field::Color));
        assert_eq!(draft.name, "Production", "the name survives a bad color");
    }

    #[test]
    fn a_category_draft_speaks_about_categories_rather_than_groups() {
        let group_id = GroupId::new();
        let mut draft = NameDraft::new(NameTarget::NewSubgroup(group_id));

        assert_eq!(draft.title(), "New category");
        assert_eq!(draft.label(), "Category name");
        assert_eq!(draft.action(), "Create category");
        assert_eq!(draft.build().unwrap_err().message, "Enter a category name.");

        draft.set_name(" Databases ".to_owned());
        assert_eq!(draft.build().expect("valid name").name, "Databases");
    }

    #[test]
    fn renaming_a_category_keeps_its_identifier() {
        let subgroup_id = SubgroupId::new();
        let draft = NameDraft::rename(NameTarget::RenameSubgroup(subgroup_id), "Grafana", None);

        assert_eq!(draft.target, NameTarget::RenameSubgroup(subgroup_id));
        assert_eq!(draft.title(), "Rename category");
        assert_eq!(draft.build().expect("valid name").name, "Grafana");
    }

    #[test]
    fn typing_in_a_field_clears_only_that_field_error() {
        let mut draft = server_draft();
        draft.error = draft.build().err();
        assert_eq!(draft.field_error(Field::Name), Some("Enter a server name."));

        draft.set(Field::Host, "api.example.com".to_owned());
        assert!(draft.field_error(Field::Name).is_some());

        draft.set(Field::Name, "API".to_owned());
        assert!(draft.field_error(Field::Name).is_none());
    }
}
