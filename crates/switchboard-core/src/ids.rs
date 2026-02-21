use std::fmt::{Display, Formatter, Result as FmtResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProfileId(pub u64);

impl Display for ProfileId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "profile:{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkspaceId(pub u64);

impl Display for WorkspaceId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "workspace:{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TabId(pub u64);

impl Display for TabId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "tab:{}", self.0)
    }
}
