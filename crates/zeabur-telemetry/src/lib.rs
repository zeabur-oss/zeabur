#![doc = include_str!("../README.md")]

mod event;
mod send;

pub use event::{
    Day, Distribution, ErrorCode, Event, Failure, Install, InstallationId, Mode, Profile, Step,
    ZeaburActive,
};
pub use send::{ENDPOINT, PROJECT_KEY, Sender, Show, Switch, do_not_track, do_not_track_in};
