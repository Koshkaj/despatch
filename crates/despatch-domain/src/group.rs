use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::validation::{validate_single_line_text, validate_trimmed_text};
use crate::{LabelColor, Launcher, LauncherId, ValidationError};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct GroupId(Uuid);

impl GroupId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for GroupId {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for GroupId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, formatter)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SubgroupId(Uuid);

impl SubgroupId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for SubgroupId {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for SubgroupId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, formatter)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Servers,
    Agents,
}

/// A category inside a group, such as `Grafana` or `Databases`. Launchers only ever live in a
/// subgroup, so a group is purely a folder of categories.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Subgroup {
    pub id: SubgroupId,
    pub name: String,
    pub items: Vec<Launcher>,
}

impl Subgroup {
    pub fn new(name: impl Into<String>) -> Result<Self, ValidationError> {
        Self::with_id(SubgroupId::new(), name)
    }

    pub fn with_id(id: SubgroupId, name: impl Into<String>) -> Result<Self, ValidationError> {
        let name = name.into();
        validate_single_line_text(&name).map_err(|violation| {
            ValidationError::InvalidSubgroupName {
                subgroup_id: id,
                violation,
            }
        })?;
        let subgroup = Self {
            id,
            name: name.trim().to_owned(),
            items: Vec::new(),
        };
        subgroup.validate(Section::Servers, true)?;
        Ok(subgroup)
    }

    /// Validates the subgroup, checking its launchers against `section` unless `names_only`.
    fn validate(&self, section: Section, names_only: bool) -> Result<(), ValidationError> {
        validate_trimmed_text(&self.name).map_err(|violation| {
            ValidationError::InvalidSubgroupName {
                subgroup_id: self.id,
                violation,
            }
        })?;
        if names_only {
            return Ok(());
        }

        let mut names = HashSet::new();
        for launcher in &self.items {
            launcher.validate()?;
            if !section.accepts(launcher) {
                return Err(ValidationError::LauncherSectionMismatch {
                    launcher_id: launcher.id(),
                    section,
                });
            }
            if !names.insert(launcher.name().to_lowercase()) {
                return Err(ValidationError::DuplicateLauncherName {
                    subgroup_id: self.id,
                    name: launcher.name().to_owned(),
                });
            }
        }
        Ok(())
    }

    pub fn add_launcher(
        &mut self,
        section: Section,
        launcher: Launcher,
    ) -> Result<(), ValidationError> {
        launcher.validate()?;
        if !section.accepts(&launcher) {
            return Err(ValidationError::LauncherSectionMismatch {
                launcher_id: launcher.id(),
                section,
            });
        }
        if self.items.iter().any(|item| item.id() == launcher.id()) {
            return Err(ValidationError::DuplicateLauncherId(launcher.id()));
        }
        if self
            .items
            .iter()
            .any(|item| item.name().to_lowercase() == launcher.name().to_lowercase())
        {
            return Err(ValidationError::DuplicateLauncherName {
                subgroup_id: self.id,
                name: launcher.name().to_owned(),
            });
        }
        self.items.push(launcher);
        Ok(())
    }

    pub fn reorder_items(&mut self, ordered_ids: &[LauncherId]) -> Result<(), ValidationError> {
        if ordered_ids.len() != self.items.len()
            || ordered_ids.iter().copied().collect::<HashSet<_>>().len() != self.items.len()
        {
            return Err(ValidationError::InvalidOrdering { entity: "launcher" });
        }

        let items_by_id: HashMap<_, _> = self
            .items
            .iter()
            .cloned()
            .map(|item| (item.id(), item))
            .collect();
        let reordered = ordered_ids
            .iter()
            .map(|id| items_by_id.get(id).cloned())
            .collect::<Option<Vec<_>>>()
            .ok_or(ValidationError::InvalidOrdering { entity: "launcher" })?;
        self.items = reordered;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Group {
    pub id: GroupId,
    pub name: String,
    pub section: Section,
    /// The label color the group is drawn in, or `None` to follow the section accent.
    ///
    /// A group's color cascades: its categories are drawn in it, and so is every launcher inside
    /// them that has not been given a color of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<LabelColor>,
    #[serde(default)]
    pub subgroups: Vec<Subgroup>,
}

impl Group {
    pub fn new(name: impl Into<String>, section: Section) -> Result<Self, ValidationError> {
        Self::with_id(GroupId::new(), name, section)
    }

    pub fn with_id(
        id: GroupId,
        name: impl Into<String>,
        section: Section,
    ) -> Result<Self, ValidationError> {
        let name = name.into();
        validate_single_line_text(&name).map_err(|violation| {
            ValidationError::InvalidGroupName {
                group_id: id,
                violation,
            }
        })?;
        let group = Self {
            id,
            name: name.trim().to_owned(),
            section,
            color: None,
            subgroups: Vec::new(),
        };
        group.validate()?;
        Ok(group)
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_trimmed_text(&self.name).map_err(|violation| {
            ValidationError::InvalidGroupName {
                group_id: self.id,
                violation,
            }
        })?;

        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for subgroup in &self.subgroups {
            subgroup.validate(self.section, false)?;
            if !ids.insert(subgroup.id) {
                return Err(ValidationError::DuplicateSubgroupId(subgroup.id));
            }
            if !names.insert(subgroup.name.to_lowercase()) {
                return Err(ValidationError::DuplicateSubgroupName {
                    group_id: self.id,
                    name: subgroup.name.clone(),
                });
            }
        }
        Ok(())
    }

    pub fn add_subgroup(&mut self, subgroup: Subgroup) -> Result<(), ValidationError> {
        if self.subgroups.iter().any(|item| item.id == subgroup.id) {
            return Err(ValidationError::DuplicateSubgroupId(subgroup.id));
        }
        if self
            .subgroups
            .iter()
            .any(|item| item.name.to_lowercase() == subgroup.name.to_lowercase())
        {
            return Err(ValidationError::DuplicateSubgroupName {
                group_id: self.id,
                name: subgroup.name,
            });
        }
        self.subgroups.push(subgroup);
        Ok(())
    }

    #[must_use]
    pub fn subgroup(&self, subgroup_id: SubgroupId) -> Option<&Subgroup> {
        self.subgroups
            .iter()
            .find(|subgroup| subgroup.id == subgroup_id)
    }

    pub fn subgroup_mut(&mut self, subgroup_id: SubgroupId) -> Option<&mut Subgroup> {
        self.subgroups
            .iter_mut()
            .find(|subgroup| subgroup.id == subgroup_id)
    }

    /// Every launcher the group holds, across all of its subgroups.
    pub fn launchers(&self) -> impl Iterator<Item = &Launcher> {
        self.subgroups.iter().flat_map(|subgroup| &subgroup.items)
    }

    #[must_use]
    pub fn launcher_count(&self) -> usize {
        self.subgroups
            .iter()
            .map(|subgroup| subgroup.items.len())
            .sum()
    }

    pub fn reorder_subgroups(&mut self, ordered_ids: &[SubgroupId]) -> Result<(), ValidationError> {
        if ordered_ids.len() != self.subgroups.len()
            || ordered_ids.iter().copied().collect::<HashSet<_>>().len() != self.subgroups.len()
        {
            return Err(ValidationError::InvalidOrdering { entity: "subgroup" });
        }

        let by_id: HashMap<_, _> = self
            .subgroups
            .iter()
            .cloned()
            .map(|subgroup| (subgroup.id, subgroup))
            .collect();
        let reordered = ordered_ids
            .iter()
            .map(|id| by_id.get(id).cloned())
            .collect::<Option<Vec<_>>>()
            .ok_or(ValidationError::InvalidOrdering { entity: "subgroup" })?;
        self.subgroups = reordered;
        Ok(())
    }
}

impl Section {
    #[must_use]
    pub fn accepts(self, launcher: &Launcher) -> bool {
        matches!(
            (self, launcher),
            (Self::Servers, Launcher::Ssh(_))
                | (Self::Agents, Launcher::Agent(_) | Launcher::Custom(_))
        )
    }
}
