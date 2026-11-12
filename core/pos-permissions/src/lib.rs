//! Deny-by-default workspace access policy.
//!
//! Authentication identifies a user and device, but never grants a workspace
//! privilege. Every operation needs an explicit scoped role.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceKind {
    Personal,
    Shared,
}

impl WorkspaceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::Shared => "shared",
        }
    }

    pub fn parse(input: &str) -> Option<Self> {
        match input {
            "personal" => Some(Self::Personal),
            "shared" => Some(Self::Shared),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    Owner,
    Editor,
    Viewer,
}

impl Role {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Editor => "editor",
            Self::Viewer => "viewer",
        }
    }

    /// Owner is derived from the workspace row and cannot be granted.
    pub fn membership(input: &str) -> Option<Self> {
        match input {
            "editor" => Some(Self::Editor),
            "viewer" => Some(Self::Viewer),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Read,
    Write,
    ManageMembers,
    DeleteWorkspace,
}

/// No implicit access from authentication or membership in another
/// workspace. Personal workspaces are accessible only to their owner.
pub fn allowed(kind: WorkspaceKind, role: Option<Role>, action: Action) -> bool {
    if kind == WorkspaceKind::Personal && role != Some(Role::Owner) {
        return false;
    }
    matches!(
        (role, action),
        (Some(Role::Owner), _)
            | (Some(Role::Editor), Action::Read | Action::Write)
            | (Some(Role::Viewer), Action::Read)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_and_operations_follow_least_privilege() {
        for kind in [WorkspaceKind::Personal, WorkspaceKind::Shared] {
            for action in [
                Action::Read,
                Action::Write,
                Action::ManageMembers,
                Action::DeleteWorkspace,
            ] {
                assert!(!allowed(kind, None, action));
                assert!(allowed(kind, Some(Role::Owner), action));
            }
        }
        assert!(allowed(
            WorkspaceKind::Shared,
            Some(Role::Viewer),
            Action::Read
        ));
        assert!(!allowed(
            WorkspaceKind::Shared,
            Some(Role::Viewer),
            Action::Write
        ));
        assert!(allowed(
            WorkspaceKind::Shared,
            Some(Role::Editor),
            Action::Write
        ));
        for role in [Role::Editor, Role::Viewer] {
            for action in [Action::ManageMembers, Action::DeleteWorkspace] {
                assert!(!allowed(WorkspaceKind::Shared, Some(role), action));
            }
            for action in [Action::Read, Action::Write, Action::ManageMembers] {
                assert!(!allowed(WorkspaceKind::Personal, Some(role), action));
            }
        }
    }

    #[test]
    fn owner_cannot_be_granted_as_a_membership() {
        assert_eq!(Role::membership("editor"), Some(Role::Editor));
        assert_eq!(Role::membership("viewer"), Some(Role::Viewer));
        for invalid in ["owner", "admin", "", "Editor", "editor ", "superuser"] {
            assert_eq!(Role::membership(invalid), None);
        }
        assert_eq!(WorkspaceKind::parse("shared"), Some(WorkspaceKind::Shared));
        assert_eq!(
            WorkspaceKind::parse("personal"),
            Some(WorkspaceKind::Personal)
        );
        assert_eq!(WorkspaceKind::parse("other"), None);
    }
}
