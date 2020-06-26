# lv2-ui – prototype for an ui crate for rust-lv2

**This repo is no longer maintained!**

The development has been transferred to
[rust-lv2](https://github.com/RustAudio/rust-lv2) itself. See
https://github.com/RustAudio/rust-lv2/pull/75


[rust-lv2](https://github.com/RustAudio/rust-lv2) are Rust bindings to write
[LV2 Plugins](https://lv2plug.in). This crate aims to eventually fill the gap
in rust-lv2 for LV2 GUIs.

## Status

Very early prototype stage. There is one example plugin, which is a simple
amplifier with a UI to set the gain and enable and disable the plugin.


## Example plugins

As in a Rust crate only one library is allowed, LV2 plugins with a GUI have to
come in two crates, one for the DSP part of the plugin, and one for the GUI.

The simple amplifier plugin can be found in the following two repos

* DSP: [ampmeter-rs.lv2](https://github.com/johannes-mueller/ampmeter-rs.lv2)
* UI: [ampmeter-rs-ui.lv2](https://github.com/johannes-mueller/ampmeter-rs-ui.lv2)

Look in those repose on how to build and install the example plugins.


## Todo

Still a lot

* LV2 UI feature discovery
* Atom ports [done]
* Many things still need to be done the right way
* Implement macro rules for the descriptor structs and port collections
* For sure a lot more
* Integrate into rust-lv
* Write actual plugins to test convenient usage
* Write the two example plugins eg-sampler and eg-scope for the lv2 book

Eventually this repo will hopefully disappear and become a part of rust-lv2.
