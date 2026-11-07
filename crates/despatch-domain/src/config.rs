use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{Group, GroupId, Launcher, LauncherId, Section, Subgroup, SubgroupId, ValidationError};

pub const CURRENT_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppConfig {
    pub schema_version: u32,
    pub revision: u64,
    pub groups: Vec<Group>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            revision: 0,
            groups: Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(ValidationError::UnsupportedSchemaVersion {
                found: self.schema_version,
                expected: CURRENT_SCHEMA_VERSION,
            });
        }

        let mut group_ids = HashSet::new();
        let mut subgroup_ids = HashSet::new();
        let mut launcher_ids = HashSet::new();
        let mut group_names = HashSet::new();
        for group in &self.groups {
            group.validate()?;
            if !group_ids.insert(group.id) {
                return Err(ValidationError::DuplicateGroupId(group.id));
            }
            if !group_names.insert((group.section, group.name.to_lowercase())) {
                return Err(ValidationError::DuplicateGroupName {
                    section: group.section,
                    name: group.name.clone(),
                });
            }
            for subgroup in &group.subgroups {
                if !subgroup_ids.insert(subgroup.id) {
                    return Err(ValidationError::DuplicateSubgroupId(subgroup.id));
                }
                for launcher in &subgroup.items {
                    if !launcher_ids.insert(launcher.id()) {
                        return Err(ValidationError::DuplicateLauncherId(launcher.id()));
                    }
                }
            }
        }
        Ok(())
    }

    /// The group and subgroup a launcher belongs to.
    #[must_use]
    pub fn locate_launcher(&self, launcher_id: LauncherId) -> Option<(&Group, &Subgroup)> {
        self.groups.iter().find_map(|group| {
            group
                .subgroups
                .iter()
                .find(|subgroup| subgroup.items.iter().any(|item| item.id() == launcher_id))
                .map(|subgroup| (group, subgroup))
        })
    }

    /// The group a subgroup belongs to.
    #[must_use]
    pub fn locate_subgroup(&self, subgroup_id: SubgroupId) -> Option<(&Group, &Subgroup)> {
        self.groups.iter().find_map(|group| {
            group
                .subgroup(subgroup_id)
                .map(|subgroup| (group, subgroup))
        })
    }

    pub fn subgroup_mut(&mut self, subgroup_id: SubgroupId) -> Option<(Section, &mut Subgroup)> {
        self.groups.iter_mut().find_map(|group| {
            let section = group.section;
            group
                .subgroup_mut(subgroup_id)
                .map(|subgroup| (section, subgroup))
        })
    }

    pub fn reorder_groups(
        &mut self,
        section: Section,
        ordered_ids: &[GroupId],
    ) -> Result<(), ValidationError> {
        let section_groups: Vec<_> = self
            .groups
            .iter()
            .filter(|group| group.section == section)
            .collect();
        if ordered_ids.len() != section_groups.len()
            || ordered_ids.iter().copied().collect::<HashSet<_>>().len() != section_groups.len()
        {
            return Err(ValidationError::InvalidOrdering { entity: "group" });
        }

        let groups_by_id: HashMap<_, _> = section_groups
            .into_iter()
            .map(|group| (group.id, group.clone()))
            .collect();
        let mut reordered = ordered_ids
            .iter()
            .map(|id| groups_by_id.get(id).cloned())
            .collect::<Option<Vec<_>>>()
            .ok_or(ValidationError::InvalidOrdering { entity: "group" })?
            .into_iter();

        for group in &mut self.groups {
            if group.section == section {
                *group = reordered
                    .next()
                    .ok_or(ValidationError::InvalidOrdering { entity: "group" })?;
            }
        }
        Ok(())
    }

    /// Moves a launcher between subgroups, or within one, without changing its identifier.
    pub fn move_launcher(
        &mut self,
        launcher_id: LauncherId,
        source_subgroup_id: SubgroupId,
        target_subgroup_id: SubgroupId,
        target_index: usize,
    ) -> Result<(), ValidationError> {
        if source_subgroup_id == target_subgroup_id {
            let (_, subgroup) = self
                .subgroup_mut(source_subgroup_id)
                .ok_or(ValidationError::SubgroupNotFound(source_subgroup_id))?;
            let launcher_index = subgroup
                .items
                .iter()
                .position(|launcher| launcher.id() == launcher_id)
                .ok_or(ValidationError::LauncherNotFound {
                    subgroup_id: source_subgroup_id,
                    launcher_id,
                })?;
            if target_index >= subgroup.items.len() && target_index != launcher_index {
                return Err(ValidationError::InvalidInsertionIndex {
                    index: target_index,
                    len: subgroup.items.len(),
                });
            }
            let launcher = subgroup.items.remove(launcher_index);
            subgroup
                .items
                .insert(target_index.min(subgroup.items.len()), launcher);
            return Ok(());
        }

        let launcher = self
            .locate_subgroup(source_subgroup_id)
            .ok_or(ValidationError::SubgroupNotFound(source_subgroup_id))?
            .1
            .items
            .iter()
            .find(|launcher| launcher.id() == launcher_id)
            .cloned()
            .ok_or(ValidationError::LauncherNotFound {
                subgroup_id: source_subgroup_id,
                launcher_id,
            })?;

        let (target_section, target) = self
            .locate_subgroup(target_subgroup_id)
            .map(|(group, subgroup)| (group.section, subgroup))
            .ok_or(ValidationError::SubgroupNotFound(target_subgroup_id))?;
        if !target_section.accepts(&launcher) {
            return Err(ValidationError::LauncherSectionMismatch {
                launcher_id,
                section: target_section,
            });
        }
        if target_index > target.items.len() {
            return Err(ValidationError::InvalidInsertionIndex {
                index: target_index,
                len: target.items.len(),
            });
        }
        if target
            .items
            .iter()
            .any(|item| item.name().to_lowercase() == launcher.name().to_lowercase())
        {
            return Err(ValidationError::DuplicateLauncherName {
                subgroup_id: target_subgroup_id,
                name: launcher.name().to_owned(),
            });
        }

        let (_, source) = self
            .subgroup_mut(source_subgroup_id)
            .ok_or(ValidationError::SubgroupNotFound(source_subgroup_id))?;
        let launcher_index = source
            .items
            .iter()
            .position(|item| item.id() == launcher_id)
            .ok_or(ValidationError::LauncherNotFound {
                subgroup_id: source_subgroup_id,
                launcher_id,
            })?;
        let launcher = source.items.remove(launcher_index);

        let (_, target) = self
            .subgroup_mut(target_subgroup_id)
            .ok_or(ValidationError::SubgroupNotFound(target_subgroup_id))?;
        target.items.insert(target_index, launcher);
        Ok(())
    }

    /// Every launcher in the config, paired with the subgroup that owns it.
    pub fn launchers(&self) -> impl Iterator<Item = (&Subgroup, &Launcher)> {
        self.groups.iter().flat_map(|group| {
            group
                .subgroups
                .iter()
                .flat_map(|subgroup| subgroup.items.iter().map(move |item| (subgroup, item)))
        })
    }
}
