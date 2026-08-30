use std::env;

use backend::Backend;
use log::debug;
use smithay::{
    reexports::wayland_server::{
        backend::{ClientData, ClientId, DisconnectReason},
        protocol::wl_surface::{self},
    },
    wayland::compositor::{
        with_surface_tree_downward, CompositorClientState, SurfaceAttributes, TraversalAction,
    },
};
use tracing_subscriber::{fmt, EnvFilter};

use crate::flutter_engine::FlutterEngine;
use crate::mouse_button_tracker::MouseButtonTracker;
use crate::state::State;

mod backend;
mod cursor;
mod flutter_engine;
mod focus;
mod gles_framebuffer_importer;
mod input_handling;
mod keyboard;
mod meta_window_state;
mod mouse_button_tracker;
mod settings;
mod state;
mod texture_swap_chain;
mod wayland;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up subscriber with both stdout and file layers
    tracing_subscriber::fmt()
        .with_timer(fmt::time::ChronoLocal::new(String::from(
            "%Y-%m-%d %H:%M:%S%.3f",
        )))
        .compact()
        .with_env_filter(EnvFilter::from_default_env()) // Log level from RUST_LOG
        .init();

    debug!("Starting Veshell");

    // Fix XWayland crash when too many file descriptors are open.
    let _ = rlimit::increase_nofile_limit(u64::MAX);

    if env::var("DISPLAY").is_ok() || env::var("WAYLAND_DISPLAY").is_ok() {
        backend::x11_client::run_x11_client();
    } else {
        backend::drm_backend::run_drm_backend();
    }

    Ok(())
}

pub struct FlutterState<BackendData: Backend + 'static> {
    pub flutter_engine: FlutterEngine<BackendData>,
    pub mouse_button_tracker: MouseButtonTracker,
}

lazy_static::lazy_static! {
    /// Process start, used as the epoch for wl_callback.done() timestamps.
    static ref START_TIME: std::time::Instant = std::time::Instant::now();
}

/// Milliseconds since process start.
///
/// wl_callback.done() requires a monotonically increasing millisecond
/// timestamp; clients use the delta between two callbacks to advance their
/// animation clock. Passing a value that never grows makes them believe no
/// time has passed.
pub fn frame_time_ms() -> u32 {
    START_TIME.elapsed().as_millis() as u32
}

pub fn send_frames_surface_tree(surface: &wl_surface::WlSurface, time: u32) {
    with_surface_tree_downward(
        surface,
        (),
        |_, _, &()| TraversalAction::DoChildren(()),
        |_surf, states, &()| {
            // the surface may not have any user_data if it is a subsurface and has not
            // yet been commited
            for callback in states
                .cached_state
                .get::<SurfaceAttributes>()
                .current()
                .frame_callbacks
                .drain(..)
            {
                callback.done(time);
            }
        },
        |_, _, &()| true,
    );
}

#[derive(Default)]
struct ClientState {
    compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {
        debug!("Client initialized {:?}", _client_id);
    }

    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {
        debug!("Client disconnected {:?} {:?}", _client_id, _reason);
    }
}
