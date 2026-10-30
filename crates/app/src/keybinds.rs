use gpui::{App, KeyBinding, actions};

// Shared keyboard actions used across views.
actions!(
    keybinds,
    [
        Cancel,
        Confirm,
        // Launcher list navigation
        CursorUp,
        CursorDown,
        PageUp,
        PageDown,
        // Editing
        Backspace,
        DeleteWordBack,
        // Cursor movement / selection
        CursorLeft,
        CursorRight,
        WordLeft,
        WordRight,
        SelectAll,
        SelectWordLeft,
        SelectWordRight,
        SelectLeft,
        SelectRight,
    ]
);

pub fn register(cx: &mut App) {
    // Launcher bindings (arrows + safe vim-ish defaults).
    cx.bind_keys([
        KeyBinding::new("escape", Cancel, Some("Launcher")),
        KeyBinding::new("enter", Confirm, Some("Launcher")),
        KeyBinding::new("up", CursorUp, Some("Launcher")),
        KeyBinding::new("down", CursorDown, Some("Launcher")),
        KeyBinding::new("pageup", PageUp, Some("Launcher")),
        KeyBinding::new("pagedown", PageDown, Some("Launcher")),
        KeyBinding::new("ctrl-k", CursorUp, Some("Launcher")),
        KeyBinding::new("ctrl-p", CursorUp, Some("Launcher")),
        KeyBinding::new("ctrl-j", CursorDown, Some("Launcher")),
        KeyBinding::new("ctrl-n", CursorDown, Some("Launcher")),
        KeyBinding::new("ctrl-u", PageUp, Some("Launcher")),
        KeyBinding::new("ctrl-d", PageDown, Some("Launcher")),
        KeyBinding::new("backspace", Backspace, Some("Launcher")),
        KeyBinding::new("ctrl-backspace", DeleteWordBack, Some("Launcher")),
        KeyBinding::new("left", CursorLeft, Some("Launcher")),
        KeyBinding::new("right", CursorRight, Some("Launcher")),
        KeyBinding::new("ctrl-left", WordLeft, Some("Launcher")),
        KeyBinding::new("ctrl-right", WordRight, Some("Launcher")),
        KeyBinding::new("ctrl-shift-left", SelectWordLeft, Some("Launcher")),
        KeyBinding::new("ctrl-shift-right", SelectWordRight, Some("Launcher")),
        KeyBinding::new("ctrl-b", WordLeft, Some("Launcher")),
        KeyBinding::new("ctrl-w", WordRight, Some("Launcher")),
        KeyBinding::new("ctrl-a", SelectAll, Some("Launcher")),
        KeyBinding::new("shift-left", SelectLeft, Some("Launcher")),
        KeyBinding::new("shift-right", SelectRight, Some("Launcher")),
        KeyBinding::new("ctrl-f", CursorRight, Some("Launcher")),
        KeyBinding::new("ctrl-h", CursorLeft, Some("Launcher")),
        KeyBinding::new("ctrl-l", CursorRight, Some("Launcher")),
    ]);

    // Every bar panel: Escape closes it.
    cx.bind_keys([KeyBinding::new("escape", Cancel, Some("Panel"))]);

    // The control center's WiFi password prompt, and the settings window's
    // value rows: same buffer, same keys.
    for context in ["ControlCenter", "Settings"] {
        cx.bind_keys(editing_keys(context));
    }
}

/// Escape, Enter and the text-editing keys, bound in one key context.
fn editing_keys(context: &'static str) -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("escape", Cancel, Some(context)),
        KeyBinding::new("enter", Confirm, Some(context)),
        KeyBinding::new("backspace", Backspace, Some(context)),
        KeyBinding::new("ctrl-backspace", DeleteWordBack, Some(context)),
        KeyBinding::new("left", CursorLeft, Some(context)),
        KeyBinding::new("right", CursorRight, Some(context)),
        KeyBinding::new("ctrl-h", CursorLeft, Some(context)),
        KeyBinding::new("ctrl-l", CursorRight, Some(context)),
        KeyBinding::new("ctrl-f", CursorRight, Some(context)),
        KeyBinding::new("ctrl-left", WordLeft, Some(context)),
        KeyBinding::new("ctrl-right", WordRight, Some(context)),
        KeyBinding::new("ctrl-b", WordLeft, Some(context)),
        KeyBinding::new("ctrl-w", WordRight, Some(context)),
        KeyBinding::new("ctrl-a", SelectAll, Some(context)),
        KeyBinding::new("shift-left", SelectLeft, Some(context)),
        KeyBinding::new("shift-right", SelectRight, Some(context)),
        KeyBinding::new("ctrl-shift-left", SelectWordLeft, Some(context)),
        KeyBinding::new("ctrl-shift-right", SelectWordRight, Some(context)),
    ]
}
