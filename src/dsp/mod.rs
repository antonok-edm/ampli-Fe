//! The plugin's digital signal processing is fully implemented within this module.
//!
//! All updates to input parameters are received through message passing to avoid thread locking
//! during audio processing. In particular, note that parameter smoothing is considered within the
//! scope of audio processing rather than state management. This module uses the `SmoothedRange`
//! struct to ensure that parameters are consistently and efficiently interpolated while minimizing
//! the number of messages passed.

use crate::plugin_state::StateUpdate;
use std::sync::mpsc::Receiver;

mod smoothed;
use smoothed::SmoothedRange;

use vst::buffer::AudioBuffer;

/// Handles all audio processing algorithms for the plugin.
pub(super) struct PluginDsp {
    amplitude_range: SmoothedRange,
    amplitude: f32,

    messages_from_params: Receiver<StateUpdate>,
}

impl PluginDsp {
    pub fn new(incoming_messages: Receiver<StateUpdate>) -> Self {
        Self {
            amplitude_range: SmoothedRange::new(0.5),
            amplitude: 1.,

            messages_from_params: incoming_messages,
        }
    }

    /// Applies any incoming state update events to the audio generation algorithm, and then writes
    /// processed audio into the output buffer.
    pub fn process(&mut self, buffer: &mut AudioBuffer<f32>) {
        // First, get any new changes to parameter ranges.
        while let Ok(message) = self.messages_from_params.try_recv() {
            match message {
                StateUpdate::SetKnob(v) => self.amplitude_range.set(v),
            }
        }

        let num_samples = buffer.samples();
        let num_channels = buffer.input_count();

        let (inputs, mut outputs) = buffer.split();
        for sample_idx in 0..num_samples {
            self.amplitude_range.process();
            if let Some(new_amplitude) = self.amplitude_range.get_new_value() {
                self.amplitude = new_amplitude;
            }
            for channel_idx in 0..num_channels {
                outputs[channel_idx][sample_idx] = inputs[channel_idx][sample_idx] * self.amplitude;
            }
        }
    }
}
