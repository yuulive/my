# my :ram:

My Minimalistic X window manager based on tinywm, inspired by qtile.

![my Screenshot](screenshot.jpg)

# prerequisites

you should have xmodmap installed

# design goals and features

  - kiss: only window management (no taskbar, system tray, ...), complex stuff should be done using other programs (rofi, ...)
  - configuration as code (like qtile, dwm, xmonad)
  - tiled by default (Binary space partitioning)
  - supports workspaces
  - supports multiple displays
  - single file (~600 LoC)

# using it

my is used/configured in rust, here is how to use it:

1. install rust and cargo https://doc.rust-lang.org/cargo/getting-started/installation.html
2. clone template project (__:warning: it is a different repository__): `git clone https://github.com/yazgoo/mymy`
3. edit src/main.rs (see comments for more details)
4. run `cargo build`, binary is available in target/debug/myumerwm

add the following to your .xinitrc :

```shell
exec /path/to/mymy mod4
```
