#![deny(missing_docs)]
//! Wrapper around [`RenderDoc`] for [`bevy`].
//!
//! Provides an easy way to register RenderDoc with a bevy [`App`].
//! Allows the user to launch the RenderDoc UI on capture, which makes
//! taking captures more convenient.
//!
//! Default hotkey for taking a capture is `F12`.
//!
//! # Examples
//!
//! ```rust, no_run
//! use bevy::prelude::*;
//! use bevy_renderdoc::*;
//!
//! App::new()
//!     .add_plugin(RenderDocPlugin) // order is important
//!     .add_plugins(DefaultPlugins)
//!     .run();
//!
use std::error::Error;

use bevy::{prelude::*, render::renderer::RenderDevice, winit::WinitWindows};
use renderdoc::*;
use sysinfo::{Pid, ProcessRefreshKind};

pub use renderdoc;

/// The RenderDoc [`Version`] this plugin uses.
pub type RenderDocVersion = V130;

/// The type of the [`NonSend`] resource used to store [`RenderDoc`] in [`bevy`].
///
/// Since the plugin may fail to initialize, the resource must be accessed via
/// an [`Option`].
///
/// # Examples
/// ```rust, no_run
/// # use bevy::prelude::*;
/// # use bevy_renderdoc::*;
/// #
/// fn modify_renderdoc(rd: Option<NonSendMut<RenderDocResource>>) {
///     if let Some(mut rd) = rd {
///         rd.set_log_file_path_template("your_path/file_prefix");
///     }
/// }
///
/// App::new()
///     .add_plugin(RenderDocPlugin)
///     .add_plugins(DefaultPlugins)
///     .add_startup_system(modify_renderdoc)
///     .run();
/// ```
pub type RenderDocResource = RenderDoc<RenderDocVersion>;

/// A plugin that enables [`RenderDoc`] for this application.
///
/// **This plugin needs to be inserted before the [`RenderPlugin`](bevy::render::RenderPlugin)**!\
/// Since the [`RenderPlugin`](bevy::render::RenderPlugin) is part of the [`DefaultPlugins`], this
/// plugin also needs to be added before that. To be safe, just add it first.
///
/// See [crate documentation](crate) for basic usage.
pub struct RenderDocPlugin;
impl Plugin for RenderDocPlugin {
    fn build(&self, app: &mut App) {
        let has_invalid_setup = app.world().contains_resource::<RenderDevice>()
            || app.world().contains_non_send::<WinitWindows>();

        if has_invalid_setup {
            app.add_systems(Startup, || {
                error!("RenderDocPlugin needs to be added before RenderPlugin!");
            });
            return;
        }

        match RenderDoc::<RenderDocVersion>::new() {
            Ok(mut rd) => {
                rd.set_capture_file_path_template("renderdoc/bevy_capture");
                rd.mask_overlay_bits(OverlayBits::NONE, OverlayBits::NONE);

                app.insert_non_send_resource(rd);
                app.add_systems(Startup, || info!("Initialized RenderDoc successfully!"));
                app.add_systems(Update, trigger_capture);
            }
            Err(e) => {
                app.add_systems(Startup, move || {
                    error!("Failed to initialize RenderDoc. Ensure RenderDoc is installed and visible from your $PATH. Error: \"{}\"", e);

                    let mut source: &dyn Error = &e;
                    while let Some(this_source) = source.source() {
                        error!("- {this_source}");
                        source = this_source;
                    }
                });
            }
        }
    }
}

fn trigger_capture(
    key: Res<ButtonInput<KeyCode>>,
    rd: NonSend<RenderDocResource>,
    mut replay_pid: Local<usize>,
    mut system: Local<sysinfo::System>,
) {
    // TODO: If a user were to change this hotkey on the RenderDoc instance
    // this could get mismatched.
    if key.just_pressed(KeyCode::F12) {
        // Avoid launching multiple instances of the replay ui
        if system
            .refresh_process_specifics(Pid::from(*replay_pid), ProcessRefreshKind::new().with_cpu())
        {
            return;
        }

        match rd.launch_replay_ui(true, None) {
            Ok(pid) => {
                *replay_pid = pid as usize;
                info!("Launching RenderDoc Replay UI");
            }
            Err(e) => error!("Failed to launch RenderDoc Replay UI: {}", e),
        }
    }
}
