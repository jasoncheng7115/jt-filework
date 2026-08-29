//! Command identity and the registry of what exists.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A stable command identifier such as `workspace.split.horizontal`.
///
/// Ids are part of the public contract: they appear in keymaps, in the command
/// palette, in scripts and in tests. Renaming one is a breaking change.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CommandId(String);

impl CommandId {
    /// Wrap an identifier.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The identifier as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for CommandId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for CommandId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Where a command belongs in menus and the palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CommandCategory {
    /// Layout: splits, panes, focus.
    Workspace,
    /// Tab management.
    Tabs,
    /// Moving between locations.
    Navigation,
    /// Acting on files.
    File,
    /// Selection and marking.
    SelectionAndMarks,
    /// Preview and viewers.
    View,
    /// Search.
    Search,
    /// AI and external agents.
    Ai,
    /// Jobs.
    Jobs,
    /// Application settings.
    Settings,
}

impl CommandCategory {
    /// Localization key for the category label.
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Workspace => "command.category.workspace",
            Self::Tabs => "command.category.tabs",
            Self::Navigation => "command.category.navigation",
            Self::File => "command.category.file",
            Self::SelectionAndMarks => "command.category.selection_marks",
            Self::View => "command.category.view",
            Self::Search => "command.category.search",
            Self::Ai => "command.category.ai",
            Self::Jobs => "command.category.jobs",
            Self::Settings => "command.category.settings",
        }
    }
}

/// What a command is, independent of how it is invoked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    id: CommandId,
    category: CommandCategory,
    label_key: &'static str,
    destructive: bool,
}

impl Command {
    /// Define a command.
    pub fn new(
        id: impl Into<CommandId>,
        category: CommandCategory,
        label_key: &'static str,
    ) -> Self {
        Self {
            id: id.into(),
            category,
            label_key,
            destructive: false,
        }
    }

    /// Mark a command as destructive, so the UI can confirm and the palette
    /// can flag it (`docs/SECURITY.md` §9).
    #[must_use]
    pub const fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    /// Identifier.
    pub const fn id(&self) -> &CommandId {
        &self.id
    }

    /// Category.
    pub const fn category(&self) -> CommandCategory {
        self.category
    }

    /// Localization key for the label. Never English text
    /// (`AGENTS.md` §11).
    pub const fn label_key(&self) -> &'static str {
        self.label_key
    }

    /// Whether invoking this can destroy data.
    pub const fn is_destructive(&self) -> bool {
        self.destructive
    }
}

/// Every command the application knows about.
#[derive(Debug, Clone, Default)]
pub struct CommandRegistry {
    commands: BTreeMap<CommandId, Command>,
}

impl CommandRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a command. Returns the previous definition, if any.
    pub fn register(&mut self, command: Command) -> Option<Command> {
        self.commands.insert(command.id().clone(), command)
    }

    /// Look up a command.
    pub fn get(&self, id: &CommandId) -> Option<&Command> {
        self.commands.get(id)
    }

    /// Whether a command exists.
    pub fn contains(&self, id: &CommandId) -> bool {
        self.commands.contains_key(id)
    }

    /// How many commands are registered.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Whether nothing is registered.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Every command, in id order.
    pub fn iter(&self) -> impl Iterator<Item = &Command> {
        self.commands.values()
    }

    /// Commands in one category.
    pub fn in_category(&self, category: CommandCategory) -> impl Iterator<Item = &Command> {
        self.commands
            .values()
            .filter(move |c| c.category() == category)
    }

    /// The baseline command set from `docs/UI_UX_SPEC.md` §7.1.
    pub fn baseline() -> Self {
        use CommandCategory as C;
        let mut registry = Self::new();
        let mut add = |id: &str, category: CommandCategory, key: &'static str| {
            registry.register(Command::new(id, category, key));
        };

        add(
            "workspace.split.horizontal",
            C::Workspace,
            "command.workspace.split.horizontal",
        );
        add(
            "workspace.split.vertical",
            C::Workspace,
            "command.workspace.split.vertical",
        );
        add(
            "workspace.pane.next",
            C::Workspace,
            "command.workspace.pane.next",
        );
        add(
            "workspace.pane.previous",
            C::Workspace,
            "command.workspace.pane.previous",
        );
        add(
            "workspace.pane.close",
            C::Workspace,
            "command.workspace.pane.close",
        );

        add("tab.new", C::Tabs, "command.tab.new");
        add("tab.close", C::Tabs, "command.tab.close");
        add("tab.reopen", C::Tabs, "command.tab.reopen");
        add("tab.duplicate", C::Tabs, "command.tab.duplicate");
        add("tab.pin", C::Tabs, "command.tab.pin");
        add("tab.next", C::Tabs, "command.tab.next");
        add("tab.previous", C::Tabs, "command.tab.previous");
        add("tab.move_to_pane", C::Tabs, "command.tab.move_to_pane");

        add("nav.up", C::Navigation, "command.nav.up");
        add("nav.back", C::Navigation, "command.nav.back");
        add("nav.forward", C::Navigation, "command.nav.forward");
        add("nav.home", C::Navigation, "command.nav.home");
        add("nav.goto", C::Navigation, "command.nav.goto");

        add("file.open", C::File, "command.file.open");
        add("file.view", C::File, "command.file.view");
        add("file.edit", C::File, "command.file.edit");
        add("file.rename", C::File, "command.file.rename");
        add("file.new_folder", C::File, "command.file.new_folder");
        add(
            "file.copy_to_target_pane",
            C::File,
            "command.file.copy_to_target_pane",
        );
        add(
            "file.move_to_target_pane",
            C::File,
            "command.file.move_to_target_pane",
        );

        add(
            "file.mark.toggle",
            C::SelectionAndMarks,
            "command.file.mark.toggle",
        );
        add(
            "file.mark.all",
            C::SelectionAndMarks,
            "command.file.mark.all",
        );
        add(
            "file.mark.none",
            C::SelectionAndMarks,
            "command.file.mark.none",
        );
        add(
            "file.mark.invert",
            C::SelectionAndMarks,
            "command.file.mark.invert",
        );

        add("preview.toggle", C::View, "command.preview.toggle");
        add("preview.quicklook", C::View, "command.preview.quicklook");

        add("search.open", C::Search, "command.search.open");
        add("search.ai", C::Search, "command.search.ai");

        add("ai.ask", C::Ai, "command.ai.ask");

        add("jobs.show", C::Jobs, "command.jobs.show");
        add("jobs.cancel_active", C::Jobs, "command.jobs.cancel_active");

        add("theme.set", C::Settings, "command.theme.set");
        add("locale.set", C::Settings, "command.locale.set");

        registry.register(Command::new("file.trash", C::File, "command.file.trash").destructive());
        registry
            .register(Command::new("file.delete", C::File, "command.file.delete").destructive());
        registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_baseline_registers_every_documented_command() {
        // docs/UI_UX_SPEC.md 7.1.
        let registry = CommandRegistry::baseline();
        for id in [
            "workspace.split.horizontal",
            "workspace.pane.close",
            "tab.move_to_pane",
            "nav.back",
            "file.copy_to_target_pane",
            "file.mark.invert",
            "preview.quicklook",
            "search.ai",
            "jobs.cancel_active",
            "locale.set",
        ] {
            assert!(
                registry.contains(&CommandId::new(id)),
                "{id} must be registered"
            );
        }
    }

    #[test]
    fn no_command_carries_english_text_as_its_label() {
        // AGENTS.md 11.
        for command in CommandRegistry::baseline().iter() {
            let key = command.label_key();
            assert!(
                key.starts_with("command."),
                "{} has a non-key label {key}",
                command.id()
            );
            assert!(
                !key.contains(' '),
                "{} looks like a sentence, not a key: {key}",
                command.id()
            );
        }
    }

    #[test]
    fn label_keys_are_unique() {
        let registry = CommandRegistry::baseline();
        let mut keys: Vec<_> = registry.iter().map(Command::label_key).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), before, "two commands share a label key");
    }

    #[test]
    fn destructive_commands_are_flagged() {
        let registry = CommandRegistry::baseline();
        assert!(registry
            .get(&CommandId::new("file.delete"))
            .unwrap()
            .is_destructive());
        assert!(registry
            .get(&CommandId::new("file.trash"))
            .unwrap()
            .is_destructive());
        assert!(!registry
            .get(&CommandId::new("nav.back"))
            .unwrap()
            .is_destructive());
    }

    #[test]
    fn categories_group_commands_for_menus_and_the_palette() {
        let registry = CommandRegistry::baseline();
        let marks: Vec<_> = registry
            .in_category(CommandCategory::SelectionAndMarks)
            .map(|c| c.id().as_str().to_string())
            .collect();
        assert_eq!(marks.len(), 4, "toggle, all, none, invert");
    }
}
