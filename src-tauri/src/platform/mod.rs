//! OS integration that only exists on Windows: Windows Hello signing and
//! desktop shortcuts. Every function has a non-Windows stub that reports
//! the feature as unavailable, so callers never need their own `cfg`.

pub mod hello;
pub mod hosts;
pub mod shortcut;
