//! The command bus.
//!
//! Everything that acts goes through here: a key chord resolved by the
//! keymap, a menu item, a palette entry, a script, a test. One entry point
//! means one place to check that a command exists, one place to log, and one
//! place where "can this be invoked without a mouse" is answered by
//! construction (`AGENTS.md` §9).

use std::collections::BTreeMap;

use jtf_core::{Error, ErrorCode};

use crate::ids::{CommandId, CommandRegistry};

/// Why a dispatch failed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DispatchError {
    /// The command is not registered.
    Unknown(CommandId),
    /// The command is registered but nothing handles it yet.
    NoHandler(CommandId),
    /// The handler refused.
    Refused {
        /// Which command.
        command: CommandId,
        /// Why, as a core error.
        reason: Error,
    },
}

impl core::fmt::Display for DispatchError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unknown(id) => write!(f, "unknown command: {id}"),
            Self::NoHandler(id) => write!(f, "no handler for command: {id}"),
            Self::Refused { command, reason } => write!(f, "{command} refused: {reason}"),
        }
    }
}

impl std::error::Error for DispatchError {}

impl From<DispatchError> for Error {
    fn from(value: DispatchError) -> Self {
        match value {
            DispatchError::Unknown(_) | DispatchError::NoHandler(_) => {
                Self::new(ErrorCode::Unsupported, value.to_string())
            }
            DispatchError::Refused { reason, .. } => reason,
        }
    }
}

/// A command handler.
///
/// Handlers take no key event and no toolkit type. That is the point: a
/// handler cannot tell whether it was invoked from a keyboard, a menu, a
/// script or a test.
pub type Handler = Box<dyn FnMut() -> Result<(), Error> + Send>;

/// Dispatches command ids to handlers.
pub struct CommandBus {
    registry: CommandRegistry,
    handlers: BTreeMap<CommandId, Handler>,
    history: Vec<CommandId>,
}

impl core::fmt::Debug for CommandBus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CommandBus")
            .field("registered", &self.registry.len())
            .field("handlers", &self.handlers.len())
            .field("dispatched", &self.history.len())
            .finish()
    }
}

impl CommandBus {
    /// A bus over a registry.
    pub fn new(registry: CommandRegistry) -> Self {
        Self {
            registry,
            handlers: BTreeMap::new(),
            history: Vec::new(),
        }
    }

    /// The command registry.
    pub const fn registry(&self) -> &CommandRegistry {
        &self.registry
    }

    /// Attach a handler.
    ///
    /// # Errors
    ///
    /// [`DispatchError::Unknown`] if the command is not registered, so a typo
    /// in a handler registration fails at startup rather than at the moment
    /// the user presses the key.
    pub fn set_handler(
        &mut self,
        command: impl Into<CommandId>,
        handler: Handler,
    ) -> Result<(), DispatchError> {
        let id = command.into();
        if !self.registry.contains(&id) {
            return Err(DispatchError::Unknown(id));
        }
        self.handlers.insert(id, handler);
        Ok(())
    }

    /// Whether a command has a handler.
    pub fn has_handler(&self, command: &CommandId) -> bool {
        self.handlers.contains_key(command)
    }

    /// Invoke a command by id.
    ///
    /// # Errors
    ///
    /// [`DispatchError`] when the command is unknown, unhandled, or the
    /// handler refuses.
    pub fn dispatch(&mut self, command: &CommandId) -> Result<(), DispatchError> {
        if !self.registry.contains(command) {
            return Err(DispatchError::Unknown(command.clone()));
        }
        let Some(handler) = self.handlers.get_mut(command) else {
            return Err(DispatchError::NoHandler(command.clone()));
        };
        self.history.push(command.clone());
        handler().map_err(|reason| DispatchError::Refused {
            command: command.clone(),
            reason,
        })
    }

    /// Commands dispatched so far, oldest first.
    ///
    /// Feeds the palette's recent-commands ordering
    /// (`docs/UI_TEST_PLAN.md` PAL-002) and makes tests able to assert what
    /// an interaction actually invoked.
    pub fn history(&self) -> &[CommandId] {
        &self.history
    }

    /// Registered commands that still have no handler.
    ///
    /// A startup check: every command the palette lists must do something.
    pub fn unhandled(&self) -> Vec<&CommandId> {
        self.registry
            .iter()
            .map(crate::ids::Command::id)
            .filter(|id| !self.handlers.contains_key(id))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::{KeyChord, Keymap};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn bus() -> CommandBus {
        CommandBus::new(CommandRegistry::baseline())
    }

    fn counting_handler() -> (Handler, Arc<AtomicUsize>) {
        let count = Arc::new(AtomicUsize::new(0));
        let inner = Arc::clone(&count);
        let handler: Handler = Box::new(move || {
            inner.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
        (handler, count)
    }

    #[test]
    fn every_command_is_invocable_without_a_key_event() {
        // AGENTS.md 9 / UI-KEY-002.
        let mut bus = bus();
        let (handler, count) = counting_handler();
        bus.set_handler("tab.new", handler).unwrap();

        bus.dispatch(&CommandId::new("tab.new")).unwrap();

        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(bus.history(), &[CommandId::new("tab.new")]);
    }

    #[test]
    fn the_full_input_path_is_keymap_then_id_then_bus() {
        let keymap = Keymap::parse("test", "primary+t = tab.new").unwrap();
        let mut bus = bus();
        let (handler, count) = counting_handler();
        bus.set_handler("tab.new", handler).unwrap();

        // A physical chord resolves to an id, and only then is anything run.
        let chord = KeyChord::parse("primary+t").unwrap();
        let id = keymap.resolve(&chord).unwrap().clone();
        bus.dispatch(&id).unwrap();

        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_typo_in_a_handler_registration_fails_at_startup() {
        let mut bus = bus();
        let (handler, _) = counting_handler();
        let err = bus.set_handler("tab.nwe", handler).unwrap_err();
        assert!(matches!(err, DispatchError::Unknown(_)));
    }

    #[test]
    fn dispatching_an_unknown_or_unhandled_command_is_an_error_not_a_panic() {
        let mut bus = bus();
        assert!(matches!(
            bus.dispatch(&CommandId::new("nope")),
            Err(DispatchError::Unknown(_))
        ));
        assert!(matches!(
            bus.dispatch(&CommandId::new("tab.new")),
            Err(DispatchError::NoHandler(_))
        ));
        assert!(
            bus.history().is_empty(),
            "nothing that did not run is recorded"
        );
    }

    #[test]
    fn a_refusing_handler_reports_the_command_and_the_core_error_code() {
        let mut bus = bus();
        let handler: Handler =
            Box::new(|| Err(Error::new(ErrorCode::PermissionDenied, "read-only volume")));
        bus.set_handler("file.rename", handler).unwrap();

        let err = bus.dispatch(&CommandId::new("file.rename")).unwrap_err();
        match &err {
            DispatchError::Refused { command, reason } => {
                assert_eq!(command.as_str(), "file.rename");
                assert_eq!(reason.code(), ErrorCode::PermissionDenied);
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
        let core: Error = err.into();
        assert_eq!(
            core.code(),
            ErrorCode::PermissionDenied,
            "the code survives conversion"
        );
    }

    #[test]
    fn unhandled_lists_what_the_palette_would_offer_but_cannot_do() {
        let mut bus = bus();
        let (handler, _) = counting_handler();
        bus.set_handler("tab.new", handler).unwrap();

        let unhandled = bus.unhandled();
        assert!(!unhandled.iter().any(|id| id.as_str() == "tab.new"));
        assert!(unhandled.iter().any(|id| id.as_str() == "tab.close"));
    }
}
