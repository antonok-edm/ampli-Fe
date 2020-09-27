//! All the logic behind the editor UI is contained within this module.
//!
//! Fundamentally, the UI is split into graphics rendering and state management in response to
//! input events, both of which are managed within the `EditorInterface` type.

use std::sync::mpsc::Receiver;

use vst_window::{EditorWindow, EventSource};

use crate::plugin_state::StateUpdate;

mod graphics;
mod state;

use super::EditorRemoteState;
pub(super) use state::InterfaceState;

/// Dimensions and layout of image assets.
mod image_consts {
    /// Original horizontal dimension of the background image, in pixels.
    pub const ORIG_BG_SIZE_X: usize = 1200;
    /// Original vertical dimension of the background image, in pixels.
    pub const ORIG_BG_SIZE_Y: usize = 800;

    /// Original radius of the knob image, in pixels.
    pub const ORIG_KNOB_RADIUS: usize = 200;
    /// Original center x-coordinate of the knob image, in pixels.
    pub const ORIG_KNOB_X: usize = 800;
    /// Original center y-coordinate of the knob image, in pixels.
    pub const ORIG_KNOB_Y: usize = 500;
}

/// Display scale of the entire UI.
const SCALE: f64 = 0.5;

/// Actual pixel width of the editor window.
pub(super) const SIZE_X: usize = (image_consts::ORIG_BG_SIZE_X as f64 * SCALE) as usize;
/// Actual pixel height of the editor window.
pub(super) const SIZE_Y: usize = (image_consts::ORIG_BG_SIZE_Y as f64 * SCALE) as usize;

/// Represents a window containing an editor interface. A new one is used each time the parent
/// window provided by the host DAW is opened or closed.
pub(super) struct EditorInterface {
    renderer: graphics::Renderer,
    event_source: EventSource,
    state: InterfaceState,
}

impl EditorInterface {
    /// Setup the `EditorInterface` within the provided parent `EditorWindow` to respond to events
    /// from the corresponding `EventSource`.
    pub fn new(
        window: EditorWindow,
        event_source: EventSource,
        initial_state: InterfaceState,
    ) -> Self {
        let renderer = graphics::Renderer::new(window);

        Self {
            renderer,
            event_source,
            state: initial_state,
        }
    }

    /// Run as much as possible of the editor interface without blocking. This means acting on any
    /// pending state change events from remote state storage, responding to any new window input
    /// events, and then rendering the new state of the UI.
    pub fn run_tasks<S: EditorRemoteState>(
        &mut self,
        remote_state: &S,
        incoming: &mut Receiver<StateUpdate>,
    ) {
        while let Ok(event) = incoming.try_recv() {
            self.state.react_to_control_event(event);
        }

        while let Some(event) = self.event_source.poll_event() {
            self.state.react_to_window_event(event, remote_state);
        }

        self.renderer.draw_frame(&self.state);
    }
}
