use std::fmt;

/// A generational identifier used by [`crate::Arena`].
///
/// The low 32 bits are the slot index and the high 32 bits are the generation.
pub trait Id: Copy + Eq + Ord + std::hash::Hash + fmt::Debug {
    fn from_raw(raw: u64) -> Self;
    fn raw(self) -> u64;
}

macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        pub struct $name(u64);

        impl $name {
            pub const fn from_raw(raw: u64) -> Self { Self(raw) }
            pub const fn raw(self) -> u64 { self.0 }
            pub const fn index(self) -> u32 { self.0 as u32 }
            pub const fn generation(self) -> u32 { (self.0 >> 32) as u32 }
            pub const fn is_nil(self) -> bool { self.0 == 0 }
        }

        impl Id for $name {
            fn from_raw(raw: u64) -> Self { Self(raw) }
            fn raw(self) -> u64 { self.0 }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "({}:{})"), self.index(), self.generation())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}:{}", self.index(), self.generation())
            }
        }
    };
}

id_type!(
    /// Identifies a buffer in the editor arena.
    BufferId
);
id_type!(
    /// Identifies a presentation-layer window.
    WindowId
);
id_type!(
    /// Identifies a tab page.
    TabId
);
id_type!(
    /// Identifies a sourced script.
    ScriptId
);
id_type!(
    /// Identifies a job.
    JobId
);
id_type!(
    /// Identifies a channel.
    ChannelId
);
id_type!(
    /// Identifies a timer.
    TimerId
);
id_type!(
    /// Identifies an autocommand group.
    AugroupId
);
id_type!(
    /// Identifies a mapping.
    MappingId
);
id_type!(
    /// Identifies a buffer-local listener.
    ListenerId
);
