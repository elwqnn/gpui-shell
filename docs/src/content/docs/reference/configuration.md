---
title: Configuration
description: Overview of GPUi Shell configuration files.
---

GPUi Shell loads configuration from:

- `$XDG_CONFIG_HOME/gpuishell/config.toml`
- or `~/.config/gpuishell/config.toml` (fallback)

If the file does not exist, it is created with defaults on startup. Changes are hot-reloaded automatically. Most settings can also be edited from the launcher with `;cfg`, which rewrites only the keys you change (see the [Config Module](/reference/launcher/#config-module)).

## `config.toml` example

```toml
[bar]
size = 32.0
position = "top"
start = ["LauncherBtn", "Workspaces", "SysInfo"]
center = ["ActiveWindow"]
end = ["Clock", "Mpris", "Notifications", "Systray", "KeyboardLayout", "ControlCenter"]

[launcher]
width = 600.0
height = 450.0
margin_top = 100.0
margin_right = 0.0
margin_bottom = 0.0
margin_left = 0.0

[osd]
position = "right"

[dock]
position = "bottom"
monitors = "primaryonly"
visibility = "intelligent-hide"
hover_effect = "lift"
icon_size = 40.0
pinned = ["firefox.desktop"]

[control_center.power_actions]
sleep = "systemctl suspend"
reboot = "systemctl reboot"
poweroff = "systemctl poweroff"
```

## Frame

The `[frame]` section draws a band around the work area with rounded inner
corners. An attached bar and a taskbar dock (`style = "attached"`,
`stretch = true`, `reserve_space = true`) become sides of the frame, as deep
as they are; a floating bar or dock sits inside it. Windows are kept inside
the frame.

```toml
[frame]
enabled = true
thickness = 6.0     # width of the band on the edges nothing sits on
# radius = 16.0     # inner corner radius; unset, it follows the theme's radius_scale
# opacity = 0.9     # unset, the band matches the bar's background
```

`bar.style = "frame"` is the old spelling of an attached bar with the frame
on, with the geometry under `[bar.frame]`; it still works.

## Dock

The `[dock]` section configures the pinned and running application dock.

```toml
[dock]
position = "bottom"       # "top", "bottom", "left", or "right"
style = "floating"        # "floating" (inset, rounded) or "attached" (flush to the edge)
stretch = false           # true spans the whole edge
align = "center"          # "start", "center" or "end": where the icons sit along the edge
launcher = false          # true adds a button at the start that opens the launcher
reserve_space = false     # true keeps windows off the dock, like the bar; it then never hides
monitors = "primaryonly"  # "primaryonly" or "all"
visibility = "intelligent-hide"
hover_effect = "lift"     # "none", "lift", "magnify", "glow", or "magnify-lift"
icon_size = 40.0
pinned = ["firefox.desktop"]
```

`style = "attached"` with `stretch = true`, `align = "start"`, `launcher =
true` and `reserve_space = true` is a taskbar: a strip along the edge that
windows tile against, apps from the left, launcher first. The default is a
floating card that fits its icons and hides when it is in the way.

Right-click an icon for its menu: a new window, the app's open windows to
jump to, pin or unpin, and close. Pinning from the menu writes `state.toml`,
which overrides `pinned` here.

`visibility` accepts (ignored while `reserve_space` is on):

- `"always-visible"`: the dock never hides.
- `"intelligent-hide"` (default): the dock hides when focus leaves its monitor and reappears at its edge.
- `"dodge-windows"`: the dock hides only while a window on its monitor's current workspace covers it, and comes back when the pointer reaches its edge or nothing is in the way. On compositors without window geometry (Niri), any window on that workspace counts as covering it.

See the individual reference pages for details on each section:

- [Bar](/gpui-shell/reference/bar/)
- [Launcher](/gpui-shell/reference/launcher/)
- [OSD](/gpui-shell/reference/osd/)
- [Control Center](/gpui-shell/reference/control-center/)
- [Theme](/gpui-shell/reference/theme/)
