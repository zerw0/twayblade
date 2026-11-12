//! Binding registry and platform-resolved data for Claude's shortcut views.
use crate::{app::*, ui::viewer::*};
use gpui_kit::KeyBinding;

gpui_kit::actions!(
    twayblade_shortcuts,
    [
        FormatBold,
        FormatItalic,
        FormatCode,
        FormatQuote,
        FormatLink,
        HistoryPrevious,
        HistoryNext,
        ShowKeyboardShortcuts,
        ToggleMute,
        ToggleCamera,
        ToggleScreenShare,
        LeaveCall,
        AnswerCall,
        DeclineCall
    ]
);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Area {
    Composer,
    Navigation,
    Window,
    Viewer,
    Calls,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    MacOS,
    Linux,
    Windows,
}
impl Platform {
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOS
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct KeyChord {
    pub mac: &'static str,
    pub other: &'static str,
}
impl KeyChord {
    /// Explicit modifiers; independent of the host used to generate/test the list.
    pub fn resolve(self, platform: Platform) -> &'static str {
        if platform == Platform::MacOS {
            self.mac
        } else {
            self.other
        }
    }
    pub fn parts(self, platform: Platform) -> Vec<String> {
        let mut source = self.resolve(platform);
        let mut parts = Vec::new();
        while let Some((modifier, rest)) = source.split_once('-') {
            let label = match (platform, modifier) {
                (Platform::MacOS, "cmd") => "⌘",
                (Platform::MacOS, "ctrl") => "⌃",
                (Platform::MacOS, "alt") => "⌥",
                (Platform::MacOS, "shift") => "⇧",
                (_, "ctrl") => "Ctrl",
                (_, "alt") => "Alt",
                (_, "shift") => "Shift",
                _ => break,
            };
            parts.push(label.to_string());
            source = rest;
        }
        // Platform order: ⌃⌥⇧⌘ on macOS, Ctrl+Alt+Shift elsewhere.
        const ORDER: [&str; 7] = ["⌃", "⌥", "⇧", "⌘", "Ctrl", "Alt", "Shift"];
        parts.sort_by_key(|part| ORDER.iter().position(|o| o == part));
        parts.push(match source {
            "enter" => "Return".into(),
            "escape" => "Escape".into(),
            "up" => "↑".into(),
            "down" => "↓".into(),
            "left" => "←".into(),
            "right" => "→".into(),
            "home" => "Home".into(),
            "end" => "End".into(),
            key => key.to_uppercase(),
        });
        parts
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortcutAction {
    OpenSettings,
    Quit,
    FindConversation,
    CloseProfile,
    ToggleSidebar,
    NavigateBack,
    NavigateForward,
    ToggleDetails,
    NewDirectMessage,
    NewRoom,
    IncreaseScale,
    DecreaseScale,
    ResetScale,
    CloseViewer,
    PreviousImage,
    NextImage,
    ZoomIn,
    ZoomOut,
    FitImage,
    SaveImage,
    CopyImage,
    OpenImage,
    RevealImage,
    FormatBold,
    FormatItalic,
    FormatCode,
    FormatQuote,
    FormatLink,
    HistoryPrevious,
    HistoryNext,
    ShowKeyboardShortcuts,
    ToggleMute,
    ToggleCamera,
    ToggleScreenShare,
    LeaveCall,
    AnswerCall,
    DeclineCall,
}
impl ShortcutAction {
    fn binding(self, keys: &'static str, context: Option<&str>) -> KeyBinding {
        match self {
            Self::OpenSettings => KeyBinding::new(keys, OpenSettings, context),
            Self::Quit => KeyBinding::new(keys, Quit, context),
            Self::FindConversation => KeyBinding::new(keys, FindConversation, context),
            Self::CloseProfile => KeyBinding::new(keys, CloseProfile, context),
            Self::ToggleSidebar => KeyBinding::new(keys, ToggleSidebar, context),
            Self::NavigateBack => KeyBinding::new(keys, NavigateBack, context),
            Self::NavigateForward => KeyBinding::new(keys, NavigateForward, context),
            Self::ToggleDetails => KeyBinding::new(keys, ToggleDetails, context),
            Self::NewDirectMessage => KeyBinding::new(keys, NewDirectMessage, context),
            Self::NewRoom => KeyBinding::new(keys, NewRoom, context),
            Self::IncreaseScale => KeyBinding::new(keys, IncreaseScale, context),
            Self::DecreaseScale => KeyBinding::new(keys, DecreaseScale, context),
            Self::ResetScale => KeyBinding::new(keys, ResetScale, context),
            Self::CloseViewer => KeyBinding::new(keys, CloseViewer, context),
            Self::PreviousImage => KeyBinding::new(keys, PreviousImage, context),
            Self::NextImage => KeyBinding::new(keys, NextImage, context),
            Self::ZoomIn => KeyBinding::new(keys, ZoomIn, context),
            Self::ZoomOut => KeyBinding::new(keys, ZoomOut, context),
            Self::FitImage => KeyBinding::new(keys, FitImage, context),
            Self::SaveImage => KeyBinding::new(keys, SaveImage, context),
            Self::CopyImage => KeyBinding::new(keys, CopyImage, context),
            Self::OpenImage => KeyBinding::new(keys, OpenImage, context),
            Self::RevealImage => KeyBinding::new(keys, RevealImage, context),
            Self::FormatBold => KeyBinding::new(keys, FormatBold, context),
            Self::FormatItalic => KeyBinding::new(keys, FormatItalic, context),
            Self::FormatCode => KeyBinding::new(keys, FormatCode, context),
            Self::FormatQuote => KeyBinding::new(keys, FormatQuote, context),
            Self::FormatLink => KeyBinding::new(keys, FormatLink, context),
            Self::HistoryPrevious => KeyBinding::new(keys, HistoryPrevious, context),
            Self::HistoryNext => KeyBinding::new(keys, HistoryNext, context),
            Self::ShowKeyboardShortcuts => KeyBinding::new(keys, ShowKeyboardShortcuts, context),
            Self::ToggleMute => KeyBinding::new(keys, ToggleMute, context),
            Self::ToggleCamera => KeyBinding::new(keys, ToggleCamera, context),
            Self::ToggleScreenShare => KeyBinding::new(keys, ToggleScreenShare, context),
            Self::LeaveCall => KeyBinding::new(keys, LeaveCall, context),
            Self::AnswerCall => KeyBinding::new(keys, AnswerCall, context),
            Self::DeclineCall => KeyBinding::new(keys, DeclineCall, context),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Shortcut {
    pub area: Area,
    pub label: &'static str,
    pub keys: KeyChord,
    /// None means provided by Kit or the existing composer behaviour, never rebound here.
    pub action: Option<ShortcutAction>,
    pub context: Option<&'static str>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShortcutEntry {
    pub area: Area,
    pub label: &'static str,
    pub keys: Vec<String>,
    pub display_only: bool,
    pub context: Option<&'static str>,
}
pub const SHORTCUTS: &[Shortcut] = &[
    Shortcut {
        area: Area::Calls,
        label: "Mute or unmute",
        keys: KeyChord {
            mac: "shift-cmd-m",
            other: "ctrl-shift-m",
        },
        action: Some(ShortcutAction::ToggleMute),
        context: Some("Twayblade"),
    },
    Shortcut {
        area: Area::Calls,
        label: "Turn camera on or off",
        keys: KeyChord {
            mac: "shift-cmd-o",
            other: "ctrl-shift-o",
        },
        action: Some(ShortcutAction::ToggleCamera),
        context: Some("Twayblade"),
    },
    Shortcut {
        area: Area::Calls,
        label: "Share screen or stop sharing",
        keys: KeyChord {
            mac: "shift-cmd-e",
            other: "ctrl-shift-e",
        },
        action: Some(ShortcutAction::ToggleScreenShare),
        context: Some("Twayblade"),
    },
    Shortcut {
        area: Area::Calls,
        label: "Leave call",
        keys: KeyChord {
            mac: "shift-cmd-h",
            other: "ctrl-shift-h",
        },
        action: Some(ShortcutAction::LeaveCall),
        context: Some("Twayblade"),
    },
    Shortcut {
        area: Area::Calls,
        label: "Answer incoming call",
        keys: KeyChord {
            mac: "shift-cmd-a",
            other: "ctrl-shift-a",
        },
        action: Some(ShortcutAction::AnswerCall),
        context: Some("Twayblade"),
    },
    Shortcut {
        area: Area::Calls,
        label: "Decline incoming call",
        keys: KeyChord {
            mac: "shift-cmd-d",
            other: "ctrl-shift-d",
        },
        action: Some(ShortcutAction::DeclineCall),
        context: Some("Twayblade"),
    },
    Shortcut {
        area: Area::Composer,
        label: "Paste",
        keys: KeyChord {
            mac: "cmd-v",
            other: "ctrl-v",
        },
        action: None,
        context: Some("Composer"),
    },
    Shortcut {
        area: Area::Composer,
        label: "Cut",
        keys: KeyChord {
            mac: "cmd-x",
            other: "ctrl-x",
        },
        action: None,
        context: Some("Composer"),
    },
    Shortcut {
        area: Area::Composer,
        label: "Copy",
        keys: KeyChord {
            mac: "cmd-c",
            other: "ctrl-c",
        },
        action: None,
        context: Some("Composer"),
    },
    Shortcut {
        area: Area::Composer,
        label: "Select all",
        keys: KeyChord {
            mac: "cmd-a",
            other: "ctrl-a",
        },
        action: None,
        context: Some("Composer"),
    },
    Shortcut {
        area: Area::Window,
        label: "Settings",
        keys: KeyChord {
            mac: "cmd-,",
            other: "ctrl-,",
        },
        action: Some(ShortcutAction::OpenSettings),
        context: None,
    },
    Shortcut {
        area: Area::Window,
        label: "Quit",
        keys: KeyChord {
            mac: "cmd-q",
            other: "ctrl-q",
        },
        action: Some(ShortcutAction::Quit),
        context: None,
    },
    Shortcut {
        area: Area::Navigation,
        label: "Find conversation",
        keys: KeyChord {
            mac: "cmd-f",
            other: "ctrl-f",
        },
        action: Some(ShortcutAction::FindConversation),
        context: Some("Twayblade"),
    },
    Shortcut {
        area: Area::Navigation,
        label: "Close panel",
        keys: KeyChord {
            mac: "escape",
            other: "escape",
        },
        action: Some(ShortcutAction::CloseProfile),
        context: Some("Twayblade"),
    },
    Shortcut {
        area: Area::Navigation,
        label: "Find conversation",
        keys: KeyChord {
            mac: "cmd-k",
            other: "ctrl-k",
        },
        action: Some(ShortcutAction::FindConversation),
        context: Some("Twayblade"),
    },
    Shortcut {
        area: Area::Navigation,
        label: "Toggle sidebar",
        keys: KeyChord {
            mac: "ctrl-cmd-s",
            other: "ctrl-alt-s",
        },
        action: Some(ShortcutAction::ToggleSidebar),
        context: Some("Twayblade"),
    },
    Shortcut {
        area: Area::Navigation,
        label: "Back",
        keys: KeyChord {
            mac: "cmd-[",
            other: "alt-left",
        },
        action: Some(ShortcutAction::NavigateBack),
        context: Some("Twayblade"),
    },
    Shortcut {
        area: Area::Navigation,
        label: "Forward",
        keys: KeyChord {
            mac: "cmd-]",
            other: "alt-right",
        },
        action: Some(ShortcutAction::NavigateForward),
        context: Some("Twayblade"),
    },
    Shortcut {
        area: Area::Navigation,
        label: "Room details",
        keys: KeyChord {
            mac: "alt-cmd-i",
            other: "ctrl-alt-i",
        },
        action: Some(ShortcutAction::ToggleDetails),
        context: Some("Twayblade"),
    },
    Shortcut {
        area: Area::Navigation,
        label: "New direct message",
        keys: KeyChord {
            mac: "cmd-n",
            other: "ctrl-n",
        },
        action: Some(ShortcutAction::NewDirectMessage),
        context: Some("Twayblade"),
    },
    Shortcut {
        area: Area::Navigation,
        label: "Create room",
        keys: KeyChord {
            mac: "cmd-shift-n",
            other: "ctrl-shift-n",
        },
        action: Some(ShortcutAction::NewRoom),
        context: Some("Twayblade"),
    },
    Shortcut {
        area: Area::Window,
        label: "Increase interface scale",
        keys: KeyChord {
            mac: "cmd-+",
            other: "ctrl-+",
        },
        action: Some(ShortcutAction::IncreaseScale),
        context: Some("Twayblade"),
    },
    Shortcut {
        area: Area::Window,
        label: "Increase interface scale",
        keys: KeyChord {
            mac: "cmd-=",
            other: "ctrl-=",
        },
        action: Some(ShortcutAction::IncreaseScale),
        context: Some("Twayblade"),
    },
    Shortcut {
        area: Area::Window,
        label: "Decrease interface scale",
        keys: KeyChord {
            mac: "cmd--",
            other: "ctrl--",
        },
        action: Some(ShortcutAction::DecreaseScale),
        context: Some("Twayblade"),
    },
    Shortcut {
        area: Area::Window,
        label: "Actual interface size",
        keys: KeyChord {
            mac: "cmd-0",
            other: "ctrl-0",
        },
        action: Some(ShortcutAction::ResetScale),
        context: Some("Twayblade"),
    },
    Shortcut {
        area: Area::Viewer,
        label: "Close image",
        keys: KeyChord {
            mac: "escape",
            other: "escape",
        },
        action: Some(ShortcutAction::CloseViewer),
        context: Some("ImageViewer"),
    },
    Shortcut {
        area: Area::Viewer,
        label: "Previous image",
        keys: KeyChord {
            mac: "left",
            other: "left",
        },
        action: Some(ShortcutAction::PreviousImage),
        context: Some("ImageViewer"),
    },
    Shortcut {
        area: Area::Viewer,
        label: "Next image",
        keys: KeyChord {
            mac: "right",
            other: "right",
        },
        action: Some(ShortcutAction::NextImage),
        context: Some("ImageViewer"),
    },
    Shortcut {
        area: Area::Viewer,
        label: "Zoom in",
        keys: KeyChord {
            mac: "cmd-+",
            other: "ctrl-+",
        },
        action: Some(ShortcutAction::ZoomIn),
        context: Some("ImageViewer"),
    },
    Shortcut {
        area: Area::Viewer,
        label: "Zoom in",
        keys: KeyChord {
            mac: "cmd-=",
            other: "ctrl-=",
        },
        action: Some(ShortcutAction::ZoomIn),
        context: Some("ImageViewer"),
    },
    Shortcut {
        area: Area::Viewer,
        label: "Zoom in",
        keys: KeyChord {
            mac: "cmd-shift-=",
            other: "ctrl-shift-=",
        },
        action: Some(ShortcutAction::ZoomIn),
        context: Some("ImageViewer"),
    },
    Shortcut {
        area: Area::Viewer,
        label: "Zoom out",
        keys: KeyChord {
            mac: "cmd--",
            other: "ctrl--",
        },
        action: Some(ShortcutAction::ZoomOut),
        context: Some("ImageViewer"),
    },
    Shortcut {
        area: Area::Viewer,
        label: "Fit to window",
        keys: KeyChord {
            mac: "cmd-0",
            other: "ctrl-0",
        },
        action: Some(ShortcutAction::FitImage),
        context: Some("ImageViewer"),
    },
    Shortcut {
        area: Area::Viewer,
        label: "Save image",
        keys: KeyChord {
            mac: "cmd-s",
            other: "ctrl-s",
        },
        action: Some(ShortcutAction::SaveImage),
        context: Some("ImageViewer"),
    },
    Shortcut {
        area: Area::Viewer,
        label: "Copy image",
        keys: KeyChord {
            mac: "cmd-c",
            other: "ctrl-c",
        },
        action: Some(ShortcutAction::CopyImage),
        context: Some("ImageViewer"),
    },
    Shortcut {
        area: Area::Viewer,
        label: "Open image",
        keys: KeyChord {
            mac: "cmd-o",
            other: "ctrl-o",
        },
        action: Some(ShortcutAction::OpenImage),
        context: Some("ImageViewer"),
    },
    Shortcut {
        area: Area::Viewer,
        label: "Show in timeline",
        keys: KeyChord {
            mac: "cmd-shift-l",
            other: "ctrl-shift-l",
        },
        action: Some(ShortcutAction::RevealImage),
        context: Some("ImageViewer"),
    },
    Shortcut {
        area: Area::Composer,
        label: "Bold",
        keys: KeyChord {
            mac: "cmd-b",
            other: "ctrl-b",
        },
        action: Some(ShortcutAction::FormatBold),
        context: Some("Composer"),
    },
    Shortcut {
        area: Area::Composer,
        label: "Italic",
        keys: KeyChord {
            mac: "cmd-i",
            other: "ctrl-i",
        },
        action: Some(ShortcutAction::FormatItalic),
        context: Some("Composer"),
    },
    Shortcut {
        area: Area::Composer,
        label: "Code",
        keys: KeyChord {
            mac: "cmd-e",
            other: "ctrl-e",
        },
        action: Some(ShortcutAction::FormatCode),
        context: Some("Composer"),
    },
    Shortcut {
        area: Area::Composer,
        label: "Quote",
        keys: KeyChord {
            // GPUI matches the typed ">" against an unshifted binding on macOS.
            mac: "cmd->",
            other: "ctrl-shift-.",
        },
        action: Some(ShortcutAction::FormatQuote),
        context: Some("Composer"),
    },
    Shortcut {
        area: Area::Composer,
        label: "Link",
        keys: KeyChord {
            mac: "cmd-shift-l",
            other: "ctrl-shift-l",
        },
        action: Some(ShortcutAction::FormatLink),
        context: Some("Composer"),
    },
    Shortcut {
        area: Area::Composer,
        label: "Previous sent message",
        keys: KeyChord {
            // Kit binds Ctrl+Alt+arrows to extra cursors on Windows, and Linux
            // desktops reserve them for workspaces.
            mac: "ctrl-alt-up",
            other: "alt-up",
        },
        action: Some(ShortcutAction::HistoryPrevious),
        context: Some("Composer"),
    },
    Shortcut {
        area: Area::Composer,
        label: "Next sent message",
        keys: KeyChord {
            mac: "ctrl-alt-down",
            other: "alt-down",
        },
        action: Some(ShortcutAction::HistoryNext),
        context: Some("Composer"),
    },
    Shortcut {
        area: Area::Window,
        label: "Keyboard shortcuts",
        keys: KeyChord {
            mac: "cmd-/",
            other: "ctrl-/",
        },
        action: Some(ShortcutAction::ShowKeyboardShortcuts),
        context: None,
    },
    Shortcut {
        area: Area::Composer,
        label: "Send message",
        keys: KeyChord {
            mac: "enter",
            other: "enter",
        },
        action: None,
        context: Some("Composer"),
    },
    Shortcut {
        area: Area::Composer,
        label: "New line",
        keys: KeyChord {
            mac: "shift-enter",
            other: "shift-enter",
        },
        action: None,
        context: Some("Composer"),
    },
    Shortcut {
        area: Area::Composer,
        label: "Cancel edit",
        keys: KeyChord {
            mac: "escape",
            other: "escape",
        },
        action: None,
        context: Some("Composer"),
    },
    Shortcut {
        area: Area::Composer,
        label: "Edit your previous message",
        keys: KeyChord {
            mac: "up",
            other: "up",
        },
        action: None,
        context: Some("Composer"),
    },
    Shortcut {
        area: Area::Composer,
        label: "Edit your next message",
        keys: KeyChord {
            mac: "down",
            other: "down",
        },
        action: None,
        context: Some("Composer"),
    },
    Shortcut {
        area: Area::Composer,
        label: "Undo",
        keys: KeyChord {
            mac: "cmd-z",
            other: "ctrl-z",
        },
        action: None,
        context: Some("Composer"),
    },
    Shortcut {
        area: Area::Composer,
        label: "Redo",
        keys: KeyChord {
            mac: "cmd-shift-z",
            other: "ctrl-y",
        },
        action: None,
        context: Some("Composer"),
    },
    Shortcut {
        area: Area::Composer,
        label: "Move to start",
        keys: KeyChord {
            mac: "cmd-up",
            other: "ctrl-home",
        },
        action: None,
        context: Some("Composer"),
    },
    Shortcut {
        area: Area::Composer,
        label: "Move to end",
        keys: KeyChord {
            mac: "cmd-down",
            other: "ctrl-end",
        },
        action: None,
        context: Some("Composer"),
    },
];
pub fn bindings() -> Vec<KeyBinding> {
    bindings_for(Platform::current())
}
pub fn bindings_for(platform: Platform) -> Vec<KeyBinding> {
    SHORTCUTS
        .iter()
        .filter_map(|s| {
            s.action
                .map(|a| a.binding(s.keys.resolve(platform), s.context))
        })
        .collect()
}
/// `label` followed by its keys for this platform, for tooltips.
pub fn hint(label: &str, action: ShortcutAction) -> String {
    let platform = Platform::current();
    match SHORTCUTS.iter().find(|s| s.action == Some(action)) {
        Some(s) => {
            let parts = s.keys.parts(platform);
            let keys = if platform == Platform::MacOS {
                parts.concat()
            } else {
                parts.join("+")
            };
            format!("{label}  {keys}")
        }
        None => label.to_owned(),
    }
}
pub fn list() -> Vec<ShortcutEntry> {
    list_for(Platform::current())
}
pub fn list_for(platform: Platform) -> Vec<ShortcutEntry> {
    SHORTCUTS
        .iter()
        .map(|s| ShortcutEntry {
            area: s.area,
            label: s.label,
            keys: s.keys.parts(platform),
            display_only: s.action.is_none(),
            context: s.context,
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_parses_without_conflicts_on_every_platform() {
        for platform in [Platform::MacOS, Platform::Linux, Platform::Windows] {
            let mut seen = std::collections::HashSet::new();
            let list = list_for(platform);
            for (s, entry) in SHORTCUTS.iter().zip(&list) {
                let parsed = KeyBinding::new(s.keys.resolve(platform), FormatBold, s.context);
                assert_eq!(parsed.keystrokes().len(), 1);
                assert!(
                    seen.insert((s.context, format!("{:?}", parsed.keystrokes()))),
                    "duplicate: {s:?}"
                );
                assert_eq!(entry.keys, s.keys.parts(platform));
                assert_eq!(entry.display_only, s.action.is_none());
                assert!(!entry.keys.iter().any(String::is_empty));
            }
            let bound = bindings_for(platform);
            assert_eq!(bound.len(), list.iter().filter(|s| !s.display_only).count());
            for (actual, s) in bound
                .iter()
                .zip(SHORTCUTS.iter().filter(|s| s.action.is_some()))
            {
                let expected = s
                    .action
                    .unwrap()
                    .binding(s.keys.resolve(platform), s.context);
                assert_eq!(actual.keystrokes(), expected.keystrokes());
                assert!(actual.action().partial_eq(expected.action()));
            }
        }
    }
    #[test]
    fn call_shortcuts_work_while_typing() {
        // The composer's context is deeper than the window's, so a shared chord
        // would be taken by the composer whenever it has focus.
        for platform in [Platform::MacOS, Platform::Linux, Platform::Windows] {
            for call in SHORTCUTS.iter().filter(|s| s.area == Area::Calls) {
                assert!(
                    !SHORTCUTS.iter().any(|s| s.context == Some("Composer")
                        && s.keys.resolve(platform) == call.keys.resolve(platform)),
                    "{} is taken by the composer",
                    call.label
                );
            }
        }
    }
    #[test]
    fn key_caps_and_italic_conflict() {
        let chord = KeyChord {
            mac: "cmd-shift-n",
            other: "ctrl-shift-n",
        };
        assert_eq!(chord.parts(Platform::MacOS), ["⇧", "⌘", "N"]);
        assert_eq!(chord.parts(Platform::Windows), ["Ctrl", "Shift", "N"]);
        assert!(
            !SHORTCUTS
                .iter()
                .any(|s| s.action == Some(ShortcutAction::ToggleDetails) && s.keys.mac == "cmd-i")
        );
        assert_eq!(
            KeyChord {
                mac: "cmd--",
                other: "ctrl--"
            }
            .parts(Platform::MacOS),
            ["⌘", "-"]
        );
    }
}
