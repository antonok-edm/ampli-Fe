# ampli-Fe

<a href="https://studiorack.github.io/studiorack-site/effects/studiorack/ampli-fe/ampli-fe" alt="Download on StudioRack">
    <img src="https://img.shields.io/badge/StudioRack-v0.1.1-brightgreen?style=flat" />
</a>

ampli-Fe is a fully cross-platform VST2 plugin written in Rust.
It works on Linux, macOS, Windows, without any conditionally compiled code.

It features a fully-customized editor UI with an interactive knob and corresponding numerical value readout.

ampli-Fe's code is well-documented and freely licensed - feel free to use it as a starting point for your next VST2 plugin!

## Functionality

![Screenshot of ampli-Fe's custom editor UI](/assets/images/readme_screenshot.png)

ampli-Fe is a VST2 effect plugin.
It can be added to tracks within a Digital Audio Workstation, or DAW.

ampli-Fe has a single knob, that can be "turned" by clicking and dragging up or down.
Turning the knob will multiply the track's playback volume by a configurable amount between 0 and 2.
The current value of the knob is displayed on the UI as a reference.

## Design overview

ampli-Fe was written to demonstrate usage of the [`vst_window`](https://crates.io/crates/vst_window) crate for custom, cross-platform plugin interfaces, along with the excellent [`vst`](https://crates.io/crates/vst) bindings for Rust.
The graphics for the editor interface are drawn using [`wgpu`](https://crates.io/crates/wgpu).

For optimal thread-safety and performance, the plugin's functionality is split between three major components.

### Plugin state management

The [`plugin_state` module](/src/plugin_state.rs) receives a subset of VST API events that can occur on both the UI thread and audio processing thread, and maintains the "ground-truth" representation of the plugin's customized parameters.
It's used to coordinate parameter changes across the host DAW, audio processing logic, and editor interface.
It uses thread-safe interior mutability to ensure that its managed memory is always consistent.

### Digital signal processing

The [`dsp` module](/src/dsp/mod.rs) processes incoming audio and returns it to the host DAW.
It runs fully on the audio processing thread, and its parameters are updated from the plugin state via message passing to avoid performance-costly locking.

### Editor interface

The [`editor` module](/src/editor/mod.rs) displays the plugin state visually and allows interactive editing of the plugin's parameters.
It runs fully on the UI thread.
While open, the editor subscribes to receive update messages from the plugin state, but it also has direct access to the plugin state to allow it to "push" changes to the rest of the plugin in real time.

## Build instructions

Running `cargo build --release` will automatically compile the correct plugin for your current OS platform.
The resulting plugin binary can be found in the `target/release` directory.

Once the plugin is compiled, you'll need to make it accessible to your DAW, which can vary by platform.

### Linux

The shared object file `target/release/libampli_fe.so` can be copied directly to the user or system VST plugin directory.
This directory may vary by distribution and host DAW, so be sure to check the documentation and settings for each.

### macOS

macOS requires an extra step to "bundle" VST plugins.
After compiling, run the included `bundle_macos.sh` script, which will generate and populate the `ampli-Fe.vst` directory.
That directory can be copied directly to the user or system VST plugin directory, usually either `~/Library/Audio/Plug-Ins/VST/` or `/Library/Audio/Plug-Ins/VST/`.

### Windows

The dynamic-link library file `target/release/ampli_fe.dll` can be copied directly to the dedicated VST plugin directory.
This directory may vary by host DAW, so be sure to check its documentation and settings.

## About the name

ampli-Fe provides a volume adjustment knob, and is written in the Rust programming language.
Its name is a contraction of "amplify" (to increase volume) and "Fe" (the chemical symbol for iron, a natural component of rust).
