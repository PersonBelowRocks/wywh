# NOTICE
This is crate is made by [pramberg], you can find the original repo [here].

[pramberg]: https://github.com/pramberg
[here]: https://github.com/pramberg/bevy_renderdoc/tree/master

# bevy_renderdoc

Bevy plugin for [RenderDoc], wrapping [renderdoc-rs].

[RenderDoc]: https://renderdoc.org/
[renderdoc-rs]: https://github.com/ebkalderon/renderdoc-rs

This plugin requires that RenderDoc be installed on the target machine, with
either `renderdoc.dll` or `librenderdoc.so` visible from your `$PATH`.

## Hotkeys
`F12`: Trigger capture

## Example

```rust
use bevy::prelude::*;
use bevy_renderdoc::*;

fn main() {
    App::new()
        .add_plugin(RenderDocPlugin) // order is important
        .add_plugins(DefaultPlugins)
        .run();
}
```
Check the [examples](/examples) for more working examples.

## License

`bevy_renderdoc` is free and open source software distributed under the terms of
either the [MIT](LICENSE-MIT) or the [Apache 2.0](LICENSE-APACHE) license, at
your option.
