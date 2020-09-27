//! ampli-Fe is a minimal yet complete VST2 plugin designed to demonstrate usage of the
//! `vst_window` crate.
//!
//! It features a fully-customized editor UI with an interactive knob and corresponding numerical
//! value readout.
//!
//! ampli-Fe's code is well-documented - feel free to use it as a starting point for your next VST2
//! plugin in Rust.

use std::sync::{mpsc::channel, Arc};

use vst::{
    api::Supported,
    buffer::AudioBuffer,
    editor::Editor,
    plugin::{CanDo, HostCallback, Info, Plugin, PluginParameters},
};

mod dsp;
use dsp::PluginDsp;

mod editor;
use editor::PluginEditor;

mod plugin_state;
use plugin_state::PluginState;

/// Top level wrapper that exposes a full `vst::Plugin` implementation.
struct AmpliFeVst {
    /// The `PluginDsp` handles all of the plugin's audio processing, and is only accessed from the
    /// audio processing thread.
    dsp: PluginDsp,

    /// The `PluginState` holds the long-term state of the plugin and distributes raw parameter
    /// updates as they occur to other parts of the plugin. It is shared on both the audio
    /// processing thread and the UI thread, and updated using thread-safe interior mutability.
    state_handle: Arc<PluginState>,

    /// The `PluginEditor` implements the plugin's custom editor interface. It's temporarily stored
    /// here until being moved to the UI thread by the first `get_editor` method call.
    editor_placeholder: Option<PluginEditor>,
}

impl AmpliFeVst {
    /// Initializes the VST plugin, along with an optional `HostCallback` handle.
    fn new_maybe_host(maybe_host: Option<HostCallback>) -> Self {
        let host = maybe_host.unwrap_or_default();

        let (to_editor, editor_recv) = channel();
        let (to_dsp, dsp_recv) = channel();

        let state_handle = Arc::new(PluginState::new(host, to_dsp, to_editor));

        let editor_placeholder = Some(PluginEditor::new(Arc::clone(&state_handle), editor_recv));

        let dsp = PluginDsp::new(dsp_recv);

        Self {
            dsp,
            state_handle,
            editor_placeholder,
        }
    }
}

/// `vst::plugin_main` requires a `Default` implementation.
impl Default for AmpliFeVst {
    fn default() -> Self {
        Self::new_maybe_host(None)
    }
}

/// Main `vst` plugin implementation.
impl Plugin for AmpliFeVst {
    fn new(host: HostCallback) -> Self {
        Self::new_maybe_host(Some(host))
    }

    fn get_info(&self) -> Info {
        /// Use a hash of a string describing this plugin to avoid unique ID conflicts.
        const UNIQUE_ID_SEED: &str = "ampli-Fe Amplitude Effect VST2 Plugin";
        static UNIQUE_ID: once_cell::sync::Lazy<i32> = once_cell::sync::Lazy::new(|| {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut s = DefaultHasher::new();
            UNIQUE_ID_SEED.hash(&mut s);
            s.finish() as i32
        });

        Info {
            name: "ampli-Fe".to_string(),
            vendor: "antonok".to_string(),
            unique_id: *UNIQUE_ID,
            inputs: 2,
            outputs: 2,
            parameters: 1,
            initial_delay: 0,
            preset_chunks: true,
            ..Info::default()
        }
    }

    fn process(&mut self, buffer: &mut AudioBuffer<f32>) {
        self.dsp.process(buffer);
    }

    fn can_do(&self, _can_do: CanDo) -> Supported {
        Supported::Maybe
    }

    fn get_parameter_object(&mut self) -> Arc<dyn PluginParameters> {
        Arc::clone(&self.state_handle) as Arc<dyn PluginParameters>
    }

    fn get_editor(&mut self) -> Option<Box<dyn Editor>> {
        self.editor_placeholder
            .take()
            .map(|editor| Box::new(editor) as Box<dyn Editor>)
    }
}

vst::plugin_main!(AmpliFeVst);
