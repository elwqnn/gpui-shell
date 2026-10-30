---
title: Control Center
description: Control center configuration reference.
---

The `[control_center]` section configures the control center panel.

## Power actions

The `[control_center.power_actions]` table sets the commands for power operations.

| Option     | Type     | Default                | Description                      |
| ---------- | -------- | ---------------------- | -------------------------------- |
| `sleep`    | `string` | `"systemctl suspend"`  | Command to run for sleep/suspend. |
| `reboot`   | `string` | `"systemctl reboot"`   | Command to run for reboot.       |
| `poweroff` | `string` | `"systemctl poweroff"` | Command to run for power off.    |

Commands run via `sh -c`. Set a command to an empty string to disable it.

## Settings window

The gear in the control center header closes the panel and opens the settings
window: a centred surface of its own, with the config's sections down the left
and that section's values beside them. It is every value `config.toml` can set.

Booleans toggle in place. A value with a fixed set of spellings shows all of
them at once - the four screen edges, the dock's hide modes, and the widgets
`bar.start` / `bar.center` / `bar.end` can name, where clicking a widget adds
or removes it from that section. Everything else is edited as the TOML it is
(`36`, `"%H:%M"`, `["a", "b"]`), and a value the config cannot hold is refused
instead of saved.

Changes go straight into `config.toml`, one line at a time: the rest of the
file - comments, key order, formatting - is left as you wrote it. A value that
differs from the shell's default shows in the accent colour, with a reset button
that removes the key from the file.

Escape backs out one step: the value being typed, then the window.

## Example

```toml
[control_center.power_actions]
sleep = "systemctl suspend"
reboot = "systemctl reboot"
poweroff = "systemctl poweroff"
```
