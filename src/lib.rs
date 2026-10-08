#![cfg_attr(not(target_os = "macos"), allow(unused))]
#[cfg(not(target_os = "macos"))]
compile_error!("Craft Library supports macOS only");

pub mod apps;
pub mod backups;
pub mod builder;
pub mod files;
pub mod hourly;
pub mod installers;
pub mod jobs;
pub mod model;
pub mod network;
pub mod platform;
pub mod scheduler;
pub mod self_update;
pub mod tools;
pub mod updates;

pub mod macos_build;
