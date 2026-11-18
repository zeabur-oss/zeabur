#![doc = include_str!("../README.md")]

mod event;
mod send;

pub use event::{
    AppChangeSubmitted, Change, ChangeId, ClusterConnected, Connection, DatabaseKind, Day,
    Distribution, DomainKind, ErrorCode, Event, Failure, Feature, FeatureUsed, Install,
    InstallationId, Mode, Operation, Profile, Rollout, RolloutFinished, Source, Step, Submitted,
    ZeaburActive,
};
pub use send::{ENDPOINT, PROJECT_KEY, Sender, Show, Switch, do_not_track, do_not_track_in};
