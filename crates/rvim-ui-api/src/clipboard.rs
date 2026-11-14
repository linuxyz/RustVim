#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RequestId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardKind {
    Unnamed,
    Selection,
    Plus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardRequest {
    Get {
        kind: ClipboardKind,
        request: RequestId,
    },
    Set {
        kind: ClipboardKind,
        data: String,
        request: RequestId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardResult {
    pub request: RequestId,
    pub data: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClipboardCaps {
    pub unnamed: bool,
    pub selection: bool,
    pub plus: bool,
}
