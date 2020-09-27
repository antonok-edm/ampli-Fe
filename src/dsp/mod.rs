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

        // To take advantage of SIMD auto-vectorization, and for consistent parameter smoothing,
        // audio is processed in "chunks" of 16 samples at a time. The number of samples requested
        // by a host will generally be a multiple of 16, although the buffer may be truncated early
        // in some cases.
        //
        // This approach is overly complex for such a simple use-case, but can be particularly
        // useful for reducing unnecessary re-computation with many parameters.
        let num_samples = buffer.samples();
        let num_chunks = num_samples / 16;
        let extra_samples = num_samples % 16;
        let num_channels = buffer.input_count();

        let (inputs, mut outputs) = buffer.split();
        for chunk_start in (0..num_chunks).map(|i| i * 16) {
            self.amplitude_range.process();

            // Prepare the chunk's base amplitude value by placing it into a 16-element array, then
            // linearly interpolate them towards the next value if the amplitude has recently been
            // changed.
            let mut chunk_amplitudes = [self.amplitude; 16];
            if let Some(amplitude_range) = self.amplitude_range.get_new_value() {
                let new_amplitude = amplitude_range * 2.;
                let per_sample_difference = (new_amplitude - self.amplitude) / 16.;
                chunk_amplitudes
                    .iter_mut()
                    .zip((0..16).map(|i| i as f32 * per_sample_difference))
                    .for_each(|(amplitude, difference)| *amplitude += difference);
                self.amplitude = new_amplitude;
            }

            // Then, calculate each output sample by multiplying each input sample by its
            // corresponding amplitude value.
            for channel in 0..num_channels {
                for (i, amplitude) in chunk_amplitudes.iter().enumerate() {
                    outputs[channel][chunk_start + i] =
                        inputs[channel][chunk_start + i] * amplitude;
                }
            }
        }

        // Finally, process the final <16 samples, if any.
        for i in 0..extra_samples {
            for channel in 0..num_channels {
                // We could precompute extra interpolated amplitude values into a rollover buffer,
                // but it's simpler to approximate by just reusing the last known amplitude value.
                outputs[channel][num_chunks * 16 + i] =
                    inputs[channel][num_chunks * 16 + i] * self.amplitude;
            }
        }
    }
}
