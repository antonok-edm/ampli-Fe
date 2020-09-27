//! In VST terminology, the editor is a graphical window that can be used to display and interact
//! with a plugin using a custom visual appearance.
//!
//! The editor interface runs fully on the UI thread. It manages an OS window through a
//! cross-platform API exposed by the `vst_window` crate. It displays the state of the plugin
//! graphically, instructs the overall plugin state to update in response to window input events,
//! and handles notifications of state updates that occur on the processing thread.

use std::sync::{mpsc::Receiver, Arc};

use vst::editor::Editor;
use vst::plugin::PluginParameters;
use vst_window::setup;

use crate::plugin_state::{PluginState, StateUpdate};

mod interface;
use interface::{EditorInterface, InterfaceState, SIZE_X, SIZE_Y};

/// Persistent VST-compatible wrapper that opens and closes an `EditorInterface`.
pub(super) struct PluginEditor {
    opened_interface: Option<EditorInterface>,
    remote_state: Arc<PluginState>,
    incoming: Receiver<StateUpdate>,
}

impl PluginEditor {
    pub fn new(remote_state: Arc<PluginState>, incoming: Receiver<StateUpdate>) -> Self {
        Self {
            opened_interface: None,
            remote_state,
            incoming,
        }
    }
}

/// `PluginEditor` responds directly to VST API calls specific to the UI thread.
impl Editor for PluginEditor {
    fn size(&self) -> (i32, i32) {
        (SIZE_X as i32, SIZE_Y as i32)
    }

    fn position(&self) -> (i32, i32) {
        (0, 0)
    }

    fn open(&mut self, parent: *mut core::ffi::c_void) -> bool {
        if self.opened_interface.is_none() {
            let (window, event_source) = setup(parent, (SIZE_X as i32, SIZE_Y as i32));
            (*self.remote_state).set_event_subscription(true);
            let initial_state = InterfaceState::new(self.remote_state.get_parameter(0));
            self.opened_interface = Some(EditorInterface::new(window, event_source, initial_state));
            true
        } else {
            false
        }
    }

    fn close(&mut self) {
        self.remote_state.set_event_subscription(false);
        drop(self.opened_interface.take());
    }

    fn is_open(&mut self) -> bool {
        self.opened_interface.is_some()
    }

    fn idle(&mut self) {
        if let Some(opened_interface) = &mut self.opened_interface {
            opened_interface.run_tasks(&*self.remote_state, &mut self.incoming);
        }
    }
}

/// The editor interface holds a handle directly to the remote VST plugin state, which should
/// implement this trait. It should be possible to update the remote state through these trait
/// methods.
///
/// Each of these methods should do three things as necessary:
///   - Update the remote long-term internal state
///   - Pass a message to the audio processing thread, instructing it to affect its algorithm
///     accordingly
///   - Notify the host DAW if any of its knobs need to be re-rendered.
pub(super) trait EditorRemoteState {
    /// While the event subscription is enabled, state update events will be sent over the
    /// `control_send` channel.
    fn set_event_subscription(&self, enabled: bool);
    /// Sets the position of the amplitude control to a new fraction of its full range between 0
    /// and 1.
    fn set_amplitude_control(&self, value: f32);
}
