//! Pluto's Code | Chat split (not upstream): which space a project belongs
//! to. Chats live in hidden projects beside the server's Scratch folder:
//! the chat space (`chats`), temporary chats (`temporary-chats`) and chat
//! projects (folders under `chat-projects`); every other project is code.
//! The desktop client's `chat_mode` and `chat_projects` follow the same
//! layout.

/// Where a project's threads are listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    Code,
    /// The chat space, whose chats are kept.
    Chats,
    /// Temporary chats, listed nowhere.
    Temporary,
    /// A named chat project.
    ChatProject,
}

impl Space {
    /// Whether the space holds chats.
    pub fn is_chat(self) -> bool {
        self != Space::Code
    }
}

/// The folder beside the Scratch folder named `folder`.
pub fn beside_scratch(scratch_root: &str, folder: &str) -> Option<String> {
    let scratch = scratch_root.trim_end_matches('/');
    let (parent, _) = scratch.rsplit_once('/')?;
    Some(format!("{parent}/{folder}"))
}

/// The space of the project rooted at `project_root`, given the server's
/// `scratchWorkspaceRoot`.
pub fn space_of(scratch_root: Option<&str>, project_root: &str) -> Space {
    let Some(scratch) = scratch_root.filter(|root| !root.trim().is_empty()) else {
        return Space::Code;
    };
    let root = project_root.trim_end_matches('/');
    let beside = |folder| beside_scratch(scratch, folder);
    if beside("chats").is_some_and(|r| r == root) {
        Space::Chats
    } else if beside("temporary-chats").is_some_and(|r| r == root) {
        Space::Temporary
    } else if beside("chat-projects").is_some_and(|r| {
        root.strip_prefix(&r)
            .is_some_and(|rest| rest.starts_with('/') && rest.len() > 1)
    }) {
        Space::ChatProject
    } else {
        Space::Code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chats_live_beside_the_scratch_folder() {
        let scratch = Some("/data/pluto/scratch");
        assert_eq!(space_of(scratch, "/data/pluto/chats"), Space::Chats);
        assert_eq!(
            space_of(scratch, "/data/pluto/temporary-chats/"),
            Space::Temporary
        );
        assert_eq!(
            space_of(scratch, "/data/pluto/chat-projects/trip"),
            Space::ChatProject
        );
        assert_eq!(space_of(scratch, "/data/pluto/chat-projects"), Space::Code);
        assert_eq!(space_of(scratch, "/src/app"), Space::Code);
        assert_eq!(space_of(None, "/data/pluto/chats"), Space::Code);
    }
}
