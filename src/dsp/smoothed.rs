/// A `SmoothedRange` will get closer to its target value by this proportion of the difference
/// between the current and target value on every `process` call.
const FILTER_FACTOR: f32 = 0.005;
/// If a `SmoothedRange`'s value is at least this close to its target, it will "snap" to the
/// target and stop smoothing.
const SMOOTH_EPSILON: f32 = 0.001;

/// Represents a value between 0. and 1. that exponentially interpolates towards a settable target
/// value whenever it is processed. Allows efficient calculation of derived values by only
/// returning values when it has been updated or smoothed.
#[derive(Clone, Default)]
pub(super) struct SmoothedRange {
    value: f32,
    target: f32,

    needs_smooth: bool,
    did_change: bool,
}

impl SmoothedRange {
    pub fn new(starting_value: f32) -> Self {
        Self {
            value: starting_value,
            target: starting_value,
            needs_smooth: false,
            did_change: true,
        }
    }

    /// Smoothes this parameter towards its target value if necessary.
    pub fn process(&mut self) {
        if self.needs_smooth {
            self.did_change = true;
            self.value += (self.target - self.value) * FILTER_FACTOR;
            if (self.value - self.target).abs() < SMOOTH_EPSILON {
                self.value = self.target;
                self.needs_smooth = false;
            }
        } else {
            self.did_change = false;
        }
    }

    /// Provides a new target to smooth towards.
    pub fn set(&mut self, value: f32) {
        self.target = value;
        self.needs_smooth = true;
        self.did_change = true;
    }

    /// Return this parameter's value if it is different from its previous value because of
    /// smoothing or updating.
    pub fn get_new_value(&mut self) -> Option<f32> {
        if self.did_change {
            Some(self.value)
        } else {
            None
        }
    }
}
