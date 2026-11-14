//! Window shortcuts only use uncaptured keys, preserving text/slider editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    Toggle,
    Next,
    Previous,
    SeekForward,
    SeekBack,
    VolumeUp,
    VolumeDown,
    Search,
    Palette,
    Escape,
}
