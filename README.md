# sioctl-rs

An interface for reading the state of [`sndio`] controls.

This crate provides a wrapper around the [`sioctl_open(3)`] APIs for reading
and watching the state of [`sndio`] controls.

An inteface to the defautl [`sndio`] device can be opened by
[`Sioctl::new()`]. The initial state of controls can be read by calling
[`Sioctl::controls()`] and callbacks for subsequent changes can be requested
via [`Sioctl::watch()`].

There is currently way to set the value of controls. If this would be useful
to you, please feel free to submit a PR.

[`sndio`]: http://www.sndio.org/
[`sioctl_open(3)`]: https://man.openbsd.org/sioctl_open.3
[`Sioctl::new()`]: struct.Sioctl.html#method.new
[`Sioctl::controls()`]: struct.Sioctl.html#method.controls
[`Sioctl::watch()`]: struct.Sioctl.html#method.watch

## Example

```rust
use sioctl::Sioctl;

fn main() {
    let s = Sioctl::new();

    // Initial state of all controls.
    for control in s.controls() {
        println!("{:?}", control);
    }

    // Watch for changes to all controls:
    let mut watcher = s.watch(|control| println!("{:?}", control));

    // ...

    // When done, call join() to shutdown watching.
    watcher.join();
}
```

A more complete example is available in [`src/bin/sioctl.rs`].

[`src/bin/sioctl.rs`]: https://github.com/mjkillough/sioctl-rs/blob/master/src/bin/sioctl.rs

## License

MIT
