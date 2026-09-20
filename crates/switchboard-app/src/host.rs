use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[cfg(target_os = "macos")]
use std::sync::{Mutex, OnceLock};
use switchboard_core::{ProfileId, TabId};

use crate::bridge::{decode_client_message, UiCommand};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UiViewId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentViewId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowSize {
    pub width: u32,
    pub height: u32,
}

impl Default for WindowSize {
    fn default() -> Self {
        Self {
            width: DEFAULT_WINDOW_WIDTH_PX,
            height: DEFAULT_WINDOW_HEIGHT_PX,
        }
    }
}

#[cfg(any(test, not(target_os = "macos")))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostEvent {
    WindowCreated {
        window_id: WindowId,
        title: String,
        size: WindowSize,
    },
    UiViewCreated {
        window_id: WindowId,
        view_id: UiViewId,
        url: String,
    },
    ContentViewCreated {
        window_id: WindowId,
        view_id: ContentViewId,
        profile_id: ProfileId,
        tab_id: TabId,
        generation: u64,
        url: String,
    },
    ContentNavigated {
        view_id: ContentViewId,
        tab_id: TabId,
        url: String,
    },
    UiMessageSent {
        payload: String,
    },
    ContentHistoryAction {
        view_id: ContentViewId,
        action: &'static str,
    },
    ContentLayoutChanged {
        primary: Option<ContentViewId>,
        secondary: Option<ContentViewId>,
        ratio_millis: u16,
        split_enabled: bool,
    },
    FocusModeChanged {
        active: bool,
    },
    ContentViewDestroyed {
        view_id: ContentViewId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    InvalidUiUrl(String),
    InvalidContentUrl(String),
    Native(String),
    #[cfg(not(target_os = "macos"))]
    UnsupportedPlatform,
}

impl Display for HostError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::InvalidUiUrl(url) => write!(f, "ui view must load app://ui, got {url}"),
            Self::InvalidContentUrl(url) => {
                write!(f, "content view cannot load privileged app url: {url}")
            }
            Self::Native(message) => write!(f, "{message}"),
            #[cfg(not(target_os = "macos"))]
            Self::UnsupportedPlatform => write!(f, "native window host is only available on macOS"),
        }
    }
}

impl Error for HostError {}

pub trait CefHost {
    type Error;

    fn create_window(&mut self, title: &str, size: WindowSize) -> Result<WindowId, Self::Error>;

    fn create_ui_view(&mut self, window_id: WindowId, url: &str) -> Result<UiViewId, Self::Error>;

    fn create_content_view(
        &mut self,
        window_id: WindowId,
        profile_id: ProfileId,
        tab_id: TabId,
        generation: u64,
        url: &str,
    ) -> Result<ContentViewId, Self::Error>;

    fn navigate_content_view(
        &mut self,
        view_id: ContentViewId,
        tab_id: TabId,
        url: &str,
    ) -> Result<(), Self::Error>;

    fn set_content_view_visible(
        &mut self,
        view_id: ContentViewId,
        visible: bool,
    ) -> Result<(), Self::Error>;

    fn layout_content_views(
        &mut self,
        primary: Option<ContentViewId>,
        secondary: Option<ContentViewId>,
        ratio: f64,
        split_enabled: bool,
    ) -> Result<(), Self::Error>;

    fn set_focus_mode(&mut self, active: bool) -> Result<(), Self::Error>;

    fn go_back(&mut self, view_id: ContentViewId) -> Result<(), Self::Error>;

    fn go_forward(&mut self, view_id: ContentViewId) -> Result<(), Self::Error>;

    fn reload(&mut self, view_id: ContentViewId) -> Result<(), Self::Error>;

    fn send_ui_message(&mut self, payload: &str) -> Result<(), Self::Error>;

    fn toggle_dev_tools_for_active_content(&mut self) -> Result<(), Self::Error>;

    #[allow(dead_code)]
    fn clear_content_view(&mut self, view_id: ContentViewId) -> Result<(), Self::Error>;

    fn destroy_content_view(&mut self, view_id: ContentViewId) -> Result<(), Self::Error>;

    fn has_live_content_browser(&self, _tab_id: TabId) -> bool {
        false
    }

    fn run_event_loop(&mut self) -> Result<(), Self::Error>;
}

pub type UiCommandHandler = Box<dyn FnMut(UiCommand) + 'static>;
pub type ContentEventHandler = Box<dyn FnMut(ContentEvent) + 'static>;
pub type WindowEventHandler = Box<dyn FnMut(WindowEvent) + 'static>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentEvent {
    UrlChanged {
        tab_id: TabId,
        generation: u64,
        url: String,
    },
    TitleChanged {
        tab_id: TabId,
        generation: u64,
        title: String,
    },
    LoadingChanged {
        tab_id: TabId,
        generation: u64,
        is_loading: bool,
    },
    MainFrameLoadSucceeded {
        tab_id: TabId,
        generation: u64,
    },
    BrowserClosed {
        tab_id: TabId,
        generation: u64,
    },
    NavigationStateChanged {
        tab_id: TabId,
        generation: u64,
        can_go_back: bool,
        can_go_forward: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowEvent {
    Resized { width: u32, height: u32 },
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Keybinding {
    modifiers: KeybindingModifiers,
    key: KeyToken,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct KeybindingModifiers {
    mod_key: bool,
    ctrl: bool,
    meta: bool,
    alt: bool,
    shift: bool,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyToken {
    Character(char),
    Space,
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy)]
struct BrowserShortcutBindings {
    close_tab: Keybinding,
    command_palette: Keybinding,
    focus_navigation: Keybinding,
    toggle_devtools: Keybinding,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiShortcutAction {
    Reload,
    CloseTab,
    CommandPalette,
    FocusNavigation,
    ToggleDevTools,
    EnterBrowserMode,
    EnterInsertMode,
    NextTab,
    PreviousTab,
    NextWorkspace,
    CompleteTab,
    SnoozeTab,
    ToggleFocus,
    RestoreCompleted,
    ToggleSplit,
}

#[cfg(target_os = "macos")]
impl UiShortcutAction {
    fn host_token(self) -> &'static str {
        match self {
            Self::Reload => "reload",
            Self::CloseTab => "close_tab",
            Self::CommandPalette => "command_palette",
            Self::FocusNavigation => "focus_navigation",
            Self::ToggleDevTools => "toggle_devtools",
            Self::EnterBrowserMode => "enter_browser_mode",
            Self::EnterInsertMode => "enter_insert_mode",
            Self::NextTab => "next_tab",
            Self::PreviousTab => "previous_tab",
            Self::NextWorkspace => "next_workspace",
            Self::CompleteTab => "complete_tab",
            Self::SnoozeTab => "snooze_tab",
            Self::ToggleFocus => "toggle_focus",
            Self::RestoreCompleted => "restore_completed",
            Self::ToggleSplit => "toggle_split",
        }
    }
}

#[cfg(target_os = "macos")]
const DEFAULT_KEYBINDING_CLOSE_TAB: Keybinding = Keybinding {
    modifiers: KeybindingModifiers {
        mod_key: true,
        ctrl: false,
        meta: false,
        alt: false,
        shift: false,
    },
    key: KeyToken::Character('w'),
};

#[cfg(target_os = "macos")]
const DEFAULT_KEYBINDING_COMMAND_PALETTE: Keybinding = Keybinding {
    modifiers: KeybindingModifiers {
        mod_key: false,
        ctrl: false,
        meta: false,
        alt: false,
        shift: false,
    },
    key: KeyToken::Space,
};

#[cfg(target_os = "macos")]
const DEFAULT_KEYBINDING_FOCUS_NAVIGATION: Keybinding = Keybinding {
    modifiers: KeybindingModifiers {
        mod_key: true,
        ctrl: false,
        meta: false,
        alt: false,
        shift: false,
    },
    key: KeyToken::Character('l'),
};

#[cfg(target_os = "macos")]
const DEFAULT_KEYBINDING_TOGGLE_DEVTOOLS: Keybinding = Keybinding {
    modifiers: KeybindingModifiers {
        mod_key: true,
        ctrl: false,
        meta: false,
        alt: false,
        shift: true,
    },
    key: KeyToken::Character('i'),
};

#[cfg(target_os = "macos")]
const DEFAULT_BROWSER_SHORTCUT_BINDINGS: BrowserShortcutBindings = BrowserShortcutBindings {
    close_tab: DEFAULT_KEYBINDING_CLOSE_TAB,
    command_palette: DEFAULT_KEYBINDING_COMMAND_PALETTE,
    focus_navigation: DEFAULT_KEYBINDING_FOCUS_NAVIGATION,
    toggle_devtools: DEFAULT_KEYBINDING_TOGGLE_DEVTOOLS,
};

thread_local! {
    static UI_COMMAND_HANDLER: RefCell<Option<UiCommandHandler>> = RefCell::new(None);
    static CONTENT_EVENT_HANDLER: RefCell<Option<ContentEventHandler>> = RefCell::new(None);
    static CONTENT_EVENT_QUEUE: RefCell<VecDeque<ContentEvent>> = RefCell::new(VecDeque::new());
    static CONTENT_EVENT_DISPATCHING: Cell<bool> = const { Cell::new(false) };
    static WINDOW_EVENT_HANDLER: RefCell<Option<WindowEventHandler>> = RefCell::new(None);
    static ACTIVE_CONTENT_TAB: RefCell<Option<TabId>> = const { RefCell::new(None) };
    #[cfg(target_os = "macos")]
    static UI_ROOT_VIEW: RefCell<ObjcId> = const { RefCell::new(std::ptr::null_mut()) };
    #[cfg(target_os = "macos")]
    static UI_SHELL_VIEW: RefCell<ObjcId> = const { RefCell::new(std::ptr::null_mut()) };
    #[cfg(target_os = "macos")]
    static UI_SHELL_BROWSER: RefCell<*mut cef_browser_t> = const { RefCell::new(std::ptr::null_mut()) };
    #[cfg(target_os = "macos")]
    static UI_SHELL_BROWSER_IDENTIFIER: Cell<Option<c_int>> = const { Cell::new(None) };
    #[cfg(target_os = "macos")]
    static ACTIVE_CONTENT_CONTAINER: RefCell<ObjcId> = const { RefCell::new(std::ptr::null_mut()) };
    #[cfg(target_os = "macos")]
    static WEBSITE_KEYBINDINGS_ACTIVE: Cell<bool> = const { Cell::new(false) };
    #[cfg(target_os = "macos")]
    static BROWSER_SHORTCUT_BINDINGS: RefCell<BrowserShortcutBindings> = const { RefCell::new(DEFAULT_BROWSER_SHORTCUT_BINDINGS) };
    #[cfg(target_os = "macos")]
    static ACTIVE_CONTENT_BROWSER: RefCell<*mut cef_browser_t> = const { RefCell::new(std::ptr::null_mut()) };
    #[cfg(target_os = "macos")]
    static CONTENT_BROWSERS_BY_TAB: RefCell<HashMap<TabId, RetainedContentBrowser>> = RefCell::new(HashMap::new());
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
struct RetainedContentBrowser {
    generation: u64,
    identifier: c_int,
    browser: *mut cef_browser_t,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrowserCallbackAction {
    Insert,
    Keep,
    RefreshWrapper,
    Reject,
}

#[cfg(target_os = "macos")]
fn browser_callback_action(
    current_generation: Option<u64>,
    incoming_generation: u64,
    exact_pointer: bool,
    same_browser: bool,
) -> BrowserCallbackAction {
    match current_generation {
        None => BrowserCallbackAction::Insert,
        Some(current) if current != incoming_generation => BrowserCallbackAction::Reject,
        Some(_) if exact_pointer => BrowserCallbackAction::Keep,
        Some(_) if same_browser => BrowserCallbackAction::RefreshWrapper,
        Some(_) => BrowserCallbackAction::Reject,
    }
}

pub fn install_ui_command_handler(handler: Option<UiCommandHandler>) {
    UI_COMMAND_HANDLER.with(|slot| {
        *slot.borrow_mut() = handler;
    });
}

pub fn install_content_event_handler(handler: Option<ContentEventHandler>) {
    CONTENT_EVENT_HANDLER.with(|slot| {
        *slot.borrow_mut() = handler;
    });
}

pub fn install_window_event_handler(handler: Option<WindowEventHandler>) {
    WINDOW_EVENT_HANDLER.with(|slot| {
        *slot.borrow_mut() = handler;
    });
}

fn emit_ui_command(command: UiCommand) {
    UI_COMMAND_HANDLER.with(|slot| {
        if let Some(handler) = slot.borrow_mut().as_mut() {
            handler(command);
        }
    });
}

#[cfg(target_os = "macos")]
fn defer_ui_command(command: UiCommand) {
    let context = Box::into_raw(Box::new(command)).cast::<c_void>();
    unsafe {
        let main_queue = std::ptr::addr_of!(_dispatch_main_q)
            .cast::<c_void>()
            .cast_mut();
        dispatch_async_f(main_queue, context, run_deferred_ui_command);
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn run_deferred_ui_command(context: *mut c_void) {
    if context.is_null() {
        return;
    }
    let command = *Box::from_raw(context.cast::<UiCommand>());
    emit_ui_command(command);
}

fn emit_content_event(event: ContentEvent) {
    CONTENT_EVENT_QUEUE.with(|queue| {
        queue.borrow_mut().push_back(event);
    });

    CONTENT_EVENT_DISPATCHING.with(|dispatching| {
        if dispatching.get() {
            return;
        }
        dispatching.set(true);

        loop {
            let next_event = CONTENT_EVENT_QUEUE.with(|queue| queue.borrow_mut().pop_front());
            let Some(next_event) = next_event else {
                break;
            };

            CONTENT_EVENT_HANDLER.with(|slot| {
                let mut handler_opt = {
                    let mut slot_ref = slot.borrow_mut();
                    slot_ref.take()
                };

                if let Some(handler) = handler_opt.as_mut() {
                    handler(next_event);
                }

                let mut slot_ref = slot.borrow_mut();
                if slot_ref.is_none() {
                    *slot_ref = handler_opt;
                }
            });
        }

        dispatching.set(false);
    });
}

fn emit_window_event(event: WindowEvent) {
    WINDOW_EVENT_HANDLER.with(|slot| {
        if let Some(handler) = slot.borrow_mut().as_mut() {
            handler(event);
        }
    });
}

#[cfg(target_os = "macos")]
fn set_active_content_tab(tab_id: Option<TabId>) {
    ACTIVE_CONTENT_TAB.with(|slot| {
        *slot.borrow_mut() = tab_id;
    });
}

#[cfg(target_os = "macos")]
fn active_content_tab() -> Option<TabId> {
    ACTIVE_CONTENT_TAB.with(|slot| *slot.borrow())
}

#[cfg(target_os = "macos")]
fn set_active_content_browser(browser: *mut cef_browser_t) {
    ACTIVE_CONTENT_BROWSER.with(|slot| {
        *slot.borrow_mut() = browser;
    });
}

#[cfg(target_os = "macos")]
fn active_content_browser() -> *mut cef_browser_t {
    ACTIVE_CONTENT_BROWSER.with(|slot| *slot.borrow())
}

#[cfg(target_os = "macos")]
unsafe fn retain_cef_browser(browser: *mut cef_browser_t) {
    if browser.is_null() {
        return;
    }
    if let Some(add_ref) = (*browser).base.add_ref {
        add_ref(&mut (*browser).base);
    }
}

#[cfg(target_os = "macos")]
unsafe fn release_cef_browser(browser: *mut cef_browser_t) {
    if browser.is_null() {
        return;
    }
    if let Some(release) = (*browser).base.release {
        release(&mut (*browser).base);
    }
}

#[cfg(target_os = "macos")]
unsafe fn remember_browser_for_tab(
    tab_id: TabId,
    generation: u64,
    browser: *mut cef_browser_t,
) -> bool {
    if browser.is_null() {
        return false;
    }
    let Some(identifier) = cef_browser_identifier(browser) else {
        return false;
    };
    let accepted = CONTENT_BROWSERS_BY_TAB.with(|slot| {
        let mut browsers = slot.borrow_mut();
        if let Some(current) = browsers.get(&tab_id).copied() {
            let action = browser_callback_action(
                Some(current.generation),
                generation,
                current.browser == browser,
                current.generation == generation && current.identifier == identifier,
            );
            match action {
                BrowserCallbackAction::Keep => return true,
                BrowserCallbackAction::RefreshWrapper => {
                    // CEF may provide a different C wrapper for the same browser.
                    // Retain the callback's current wrapper before releasing the
                    // previously retained wrapper used by host actions.
                    retain_cef_browser(browser);
                    browsers.insert(
                        tab_id,
                        RetainedContentBrowser {
                            generation,
                            identifier,
                            browser,
                        },
                    );
                    release_cef_browser(current.browser);
                    return true;
                }
                BrowserCallbackAction::Reject => {}
                BrowserCallbackAction::Insert => unreachable!(),
            }
            // A tab must never own two live browsers. Keep the retained
            // browser until its OnBeforeClose callback removes it; callbacks
            // from a replacement/obsolete generation must not steal routing.
            eprintln!(
                "switchboard-app: ignored duplicate live CEF browser for tab {}",
                tab_id.0
            );
            return false;
        }
        debug_assert_eq!(
            browser_callback_action(None, generation, false, false),
            BrowserCallbackAction::Insert
        );
        retain_cef_browser(browser);
        browsers.insert(
            tab_id,
            RetainedContentBrowser {
                generation,
                identifier,
                browser,
            },
        );
        true
    });
    if accepted && active_content_tab() == Some(tab_id) {
        set_active_content_browser(browser_for_tab(tab_id));
    }
    accepted
}

#[cfg(target_os = "macos")]
fn browser_for_tab(tab_id: TabId) -> *mut cef_browser_t {
    CONTENT_BROWSERS_BY_TAB
        .with(|slot| slot.borrow().get(&tab_id).map(|entry| entry.browser))
        .unwrap_or(std::ptr::null_mut())
}

#[cfg(target_os = "macos")]
unsafe fn content_browser_matches(
    tab_id: TabId,
    generation: u64,
    browser: *mut cef_browser_t,
) -> bool {
    let Some(identifier) = cef_browser_identifier(browser) else {
        return false;
    };
    CONTENT_BROWSERS_BY_TAB.with(|slot| {
        slot.borrow()
            .get(&tab_id)
            .is_some_and(|entry| entry.generation == generation && entry.identifier == identifier)
    })
}

#[cfg(target_os = "macos")]
unsafe fn forget_browser_for_tab(tab_id: TabId, generation: u64) {
    CONTENT_BROWSERS_BY_TAB.with(|slot| {
        let should_remove = slot
            .borrow()
            .get(&tab_id)
            .is_some_and(|entry| entry.generation == generation);
        if should_remove {
            if let Some(entry) = slot.borrow_mut().remove(&tab_id) {
                release_cef_browser(entry.browser);
            }
        }
    });
}

#[cfg(target_os = "macos")]
unsafe fn set_ui_shell_browser(browser: *mut cef_browser_t) {
    let identifier = cef_browser_identifier(browser);
    UI_SHELL_BROWSER_IDENTIFIER.with(|slot| slot.set(identifier));
    UI_SHELL_BROWSER.with(|slot| {
        let mut current = slot.borrow_mut();
        if *current == browser {
            return;
        }
        let previous = std::mem::replace(&mut *current, browser);
        retain_cef_browser(browser);
        release_cef_browser(previous);
    });
}

#[cfg(target_os = "macos")]
unsafe fn remember_ui_shell_browser(browser: *mut cef_browser_t) {
    if browser.is_null() {
        return;
    }
    if ui_shell_browser().is_null() {
        set_ui_shell_browser(browser);
    }
}

#[cfg(target_os = "macos")]
fn ui_shell_browser() -> *mut cef_browser_t {
    UI_SHELL_BROWSER.with(|slot| *slot.borrow())
}

#[cfg(target_os = "macos")]
unsafe fn cef_browser_identifier(browser: *mut cef_browser_t) -> Option<c_int> {
    if browser.is_null() {
        return None;
    }
    (*browser).get_identifier.map(|callback| callback(browser))
}

#[cfg(target_os = "macos")]
unsafe fn ui_shell_browser_matches(browser: *mut cef_browser_t) -> bool {
    let Some(identifier) = cef_browser_identifier(browser) else {
        return false;
    };
    UI_SHELL_BROWSER_IDENTIFIER.with(|slot| slot.get() == Some(identifier))
}

#[cfg(target_os = "macos")]
fn set_browser_shortcut_bindings(
    close_tab: &str,
    command_palette: &str,
    focus_navigation: &str,
    toggle_devtools: &str,
) {
    let parsed = BrowserShortcutBindings {
        close_tab: parse_keybinding(close_tab).unwrap_or(DEFAULT_KEYBINDING_CLOSE_TAB),
        command_palette: parse_keybinding(command_palette)
            .unwrap_or(DEFAULT_KEYBINDING_COMMAND_PALETTE),
        focus_navigation: parse_keybinding(focus_navigation)
            .unwrap_or(DEFAULT_KEYBINDING_FOCUS_NAVIGATION),
        toggle_devtools: parse_keybinding(toggle_devtools)
            .unwrap_or(DEFAULT_KEYBINDING_TOGGLE_DEVTOOLS),
    };
    BROWSER_SHORTCUT_BINDINGS.with(|slot| {
        *slot.borrow_mut() = parsed;
    });
}

#[cfg(target_os = "macos")]
fn browser_shortcut_bindings() -> BrowserShortcutBindings {
    BROWSER_SHORTCUT_BINDINGS.with(|slot| *slot.borrow())
}

#[cfg(target_os = "macos")]
fn parse_keybinding(value: &str) -> Option<Keybinding> {
    let raw = value.trim().to_ascii_lowercase();
    if raw.is_empty() {
        return None;
    }
    let mut modifiers = KeybindingModifiers::default();
    let mut key = None;
    for part in raw
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        match part {
            "mod" => modifiers.mod_key = true,
            "ctrl" => modifiers.ctrl = true,
            "meta" => modifiers.meta = true,
            "alt" => modifiers.alt = true,
            "shift" => modifiers.shift = true,
            _ => {
                if key.is_some() {
                    return None;
                }
                key = parse_key_token(part);
            }
        }
    }
    Some(Keybinding {
        modifiers,
        key: key?,
    })
}

#[cfg(target_os = "macos")]
fn parse_key_token(value: &str) -> Option<KeyToken> {
    let normalized = match value {
        "spacebar" => "space",
        "esc" => "escape",
        "return" => "enter",
        other => other,
    };
    match normalized {
        "space" => Some(KeyToken::Space),
        "enter" => Some(KeyToken::Enter),
        "escape" => Some(KeyToken::Escape),
        "tab" => Some(KeyToken::Tab),
        "backspace" => Some(KeyToken::Backspace),
        "delete" => Some(KeyToken::Delete),
        "arrowup" => Some(KeyToken::ArrowUp),
        "arrowdown" => Some(KeyToken::ArrowDown),
        "arrowleft" => Some(KeyToken::ArrowLeft),
        "arrowright" => Some(KeyToken::ArrowRight),
        "home" => Some(KeyToken::Home),
        "end" => Some(KeyToken::End),
        "pageup" => Some(KeyToken::PageUp),
        "pagedown" => Some(KeyToken::PageDown),
        _ => {
            let mut chars = normalized.chars();
            let ch = chars.next()?;
            if chars.next().is_some() || ch.is_whitespace() {
                return None;
            }
            Some(KeyToken::Character(ch.to_ascii_lowercase()))
        }
    }
}

#[cfg(target_os = "macos")]
fn keybinding_matches_event(binding: Keybinding, event: &cef_key_event_t_switchboard) -> bool {
    let Some(event_key) = event_key_token(event) else {
        return false;
    };
    if event_key != binding.key {
        return false;
    }

    let has_ctrl = (event.modifiers & CEF_EVENTFLAG_CONTROL_DOWN) != 0;
    let has_meta = (event.modifiers & CEF_EVENTFLAG_COMMAND_DOWN) != 0;
    let has_alt = (event.modifiers & CEF_EVENTFLAG_ALT_DOWN) != 0;
    let has_shift = (event.modifiers & CEF_EVENTFLAG_SHIFT_DOWN) != 0;
    let has_primary = has_ctrl || has_meta;

    if binding.modifiers.mod_key {
        if !has_primary {
            return false;
        }
    } else if has_primary {
        return false;
    }

    if binding.modifiers.ctrl {
        if !has_ctrl {
            return false;
        }
    } else if !binding.modifiers.mod_key && has_ctrl {
        return false;
    }

    if binding.modifiers.meta {
        if !has_meta {
            return false;
        }
    } else if !binding.modifiers.mod_key && has_meta {
        return false;
    }

    if binding.modifiers.alt {
        if !has_alt {
            return false;
        }
    } else if has_alt {
        return false;
    }

    if binding.modifiers.shift {
        if !has_shift {
            return false;
        }
    } else if has_shift {
        return false;
    }

    true
}

#[cfg(target_os = "macos")]
fn event_key_token(event: &cef_key_event_t_switchboard) -> Option<KeyToken> {
    match event.windows_key_code {
        VK_BACK => Some(KeyToken::Backspace),
        VK_TAB => Some(KeyToken::Tab),
        VK_RETURN => Some(KeyToken::Enter),
        VK_ESCAPE => Some(KeyToken::Escape),
        VK_SPACE => Some(KeyToken::Space),
        VK_PRIOR => Some(KeyToken::PageUp),
        VK_NEXT => Some(KeyToken::PageDown),
        VK_END => Some(KeyToken::End),
        VK_HOME => Some(KeyToken::Home),
        VK_LEFT => Some(KeyToken::ArrowLeft),
        VK_UP => Some(KeyToken::ArrowUp),
        VK_RIGHT => Some(KeyToken::ArrowRight),
        VK_DOWN => Some(KeyToken::ArrowDown),
        VK_DELETE => Some(KeyToken::Delete),
        VK_0..=VK_9 => {
            let code = u32::try_from(event.windows_key_code).ok()?;
            let ch = char::from_u32(code)?.to_ascii_lowercase();
            Some(KeyToken::Character(ch))
        }
        VK_A..=VK_Z => {
            let code = u32::try_from(event.windows_key_code).ok()?;
            let ch = char::from_u32(code)?.to_ascii_lowercase();
            Some(KeyToken::Character(ch))
        }
        _ => {
            let unmodified = char::from_u32(u32::from(event.unmodified_character))
                .filter(|ch| !ch.is_whitespace())
                .map(|ch| ch.to_ascii_lowercase());
            if let Some(ch) = unmodified {
                return Some(KeyToken::Character(ch));
            }
            char::from_u32(u32::from(event.character))
                .filter(|ch| !ch.is_whitespace())
                .map(|ch| KeyToken::Character(ch.to_ascii_lowercase()))
        }
    }
}

#[cfg(target_os = "macos")]
fn fallback_shortcut_action(event: &cef_key_event_t_switchboard) -> Option<UiShortcutAction> {
    let token = event_key_token(event)?;
    let modifiers = event.modifiers
        & (CEF_EVENTFLAG_SHIFT_DOWN
            | CEF_EVENTFLAG_CONTROL_DOWN
            | CEF_EVENTFLAG_ALT_DOWN
            | CEF_EVENTFLAG_COMMAND_DOWN);
    if modifiers == CEF_EVENTFLAG_COMMAND_DOWN && token == KeyToken::Character('r') {
        return Some(UiShortcutAction::Reload);
    }
    let bindings = browser_shortcut_bindings();
    if keybinding_matches_event(bindings.command_palette, event) {
        return Some(UiShortcutAction::CommandPalette);
    }
    if keybinding_matches_event(bindings.focus_navigation, event) {
        return Some(UiShortcutAction::FocusNavigation);
    }
    if keybinding_matches_event(bindings.close_tab, event) {
        return Some(UiShortcutAction::CloseTab);
    }
    if keybinding_matches_event(bindings.toggle_devtools, event) {
        return Some(UiShortcutAction::ToggleDevTools);
    }
    if modifiers == CEF_EVENTFLAG_SHIFT_DOWN && token == KeyToken::Space {
        return Some(UiShortcutAction::ToggleSplit);
    }
    if modifiers != 0 {
        return None;
    }
    match token {
        KeyToken::Escape => Some(UiShortcutAction::EnterBrowserMode),
        KeyToken::Character('i') => Some(UiShortcutAction::EnterInsertMode),
        KeyToken::Character('j') => Some(UiShortcutAction::NextTab),
        KeyToken::Character('k') => Some(UiShortcutAction::PreviousTab),
        KeyToken::Character('w') => Some(UiShortcutAction::NextWorkspace),
        KeyToken::Character('d') => Some(UiShortcutAction::CompleteTab),
        KeyToken::Character('h') => Some(UiShortcutAction::SnoozeTab),
        KeyToken::Character('f') => Some(UiShortcutAction::ToggleFocus),
        KeyToken::Character('z') => Some(UiShortcutAction::RestoreCompleted),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
fn dispatch_ui_shortcut_action(action: UiShortcutAction) -> bool {
    let browser = ui_shell_browser();
    if browser.is_null() {
        return false;
    }
    unsafe {
        let Some(get_main_frame) = (*browser).get_main_frame else {
            return false;
        };
        let frame = get_main_frame(browser);
        if frame.is_null() {
            return false;
        }
        let Some(execute_js) = (*frame).execute_java_script else {
            return false;
        };
        let script = format!(
            "window.__switchboardHostShortcut && window.__switchboardHostShortcut(\"{}\");",
            action.host_token()
        );
        with_stack_cef_string(&script, |code| {
            with_stack_cef_string("app://ui/host-shortcut", |script_url| {
                execute_js(frame, code, script_url, 0);
            });
        });
    }
    true
}

#[cfg(target_os = "macos")]
fn set_ui_view_handles(root_view: ObjcId, ui_view: ObjcId) {
    UI_ROOT_VIEW.with(|slot| {
        *slot.borrow_mut() = root_view;
    });
    UI_SHELL_VIEW.with(|slot| {
        *slot.borrow_mut() = ui_view;
    });
}

#[cfg(target_os = "macos")]
fn set_ui_overlay_visible(visible: bool) -> Result<(), HostError> {
    let root_view = UI_ROOT_VIEW.with(|slot| *slot.borrow());
    let ui_view = UI_SHELL_VIEW.with(|slot| *slot.borrow());
    if root_view == NIL || ui_view == NIL {
        return Ok(());
    }
    unsafe {
        msg_send_void_id_i64_id(
            root_view,
            selector("addSubview:positioned:relativeTo:")?,
            ui_view,
            if visible { 1 } else { -1 },
            NIL,
        );
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn set_website_keybindings_active(active: bool) -> Result<(), HostError> {
    let changed = WEBSITE_KEYBINDINGS_ACTIVE.with(|slot| {
        if slot.get() == active {
            false
        } else {
            slot.set(active);
            true
        }
    });
    if !changed {
        return Ok(());
    }
    let container = ACTIVE_CONTENT_CONTAINER.with(|slot| *slot.borrow());
    if container == NIL {
        return Ok(());
    }
    apply_content_keybindings_border(container, active)
}

#[cfg(target_os = "macos")]
fn set_active_content_container(container: ObjcId) -> Result<(), HostError> {
    let previous = ACTIVE_CONTENT_CONTAINER.with(|slot| {
        let mut slot_ref = slot.borrow_mut();
        let previous = *slot_ref;
        *slot_ref = container;
        previous
    });
    if previous == container {
        return Ok(());
    }
    if previous != NIL {
        apply_content_keybindings_border(previous, false)?;
    }
    if container != NIL {
        let active = WEBSITE_KEYBINDINGS_ACTIVE.with(|slot| slot.get());
        apply_content_keybindings_border(container, active)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn clear_active_content_container_if_matches(container: ObjcId) -> Result<(), HostError> {
    let is_active = ACTIVE_CONTENT_CONTAINER.with(|slot| *slot.borrow() == container);
    if !is_active {
        return Ok(());
    }
    set_active_content_container(NIL)
}

#[cfg(target_os = "macos")]
fn apply_content_keybindings_border(container: ObjcId, active: bool) -> Result<(), HostError> {
    if container == NIL {
        return Ok(());
    }
    unsafe {
        msg_send_void_bool(container, selector("setWantsLayer:")?, YES);
        let layer = msg_send_id(container, selector("layer")?);
        if layer == NIL {
            return Ok(());
        }
        msg_send_void_bool(
            layer,
            selector("setMasksToBounds:")?,
            if active { YES } else { NO },
        );
        msg_send_void_f64(
            layer,
            selector("setCornerRadius:")?,
            if active {
                WEBSITE_KEYBINDINGS_BORDER_RADIUS_PX
            } else {
                0.0
            },
        );
        msg_send_void_f64(
            layer,
            selector("setBorderWidth:")?,
            if active {
                WEBSITE_KEYBINDINGS_BORDER_WIDTH_PX
            } else {
                0.0
            },
        );
        if active {
            let ns_color = objc_class("NSColor")?;
            let border_color = msg_send_id(ns_color, selector("systemPinkColor")?);
            if border_color != NIL {
                let cg_color = msg_send_id(border_color, selector("CGColor")?);
                if cg_color != NIL {
                    msg_send_void_id(layer, selector("setBorderColor:")?, cg_color);
                }
            }
        }
    }
    Ok(())
}

#[cfg(any(test, not(target_os = "macos")))]
#[derive(Debug, Default)]
pub struct MockCefHost {
    next_window_id: u64,
    next_ui_view_id: u64,
    next_content_view_id: u64,
    events: Vec<HostEvent>,
}

#[cfg(any(test, not(target_os = "macos")))]
impl MockCefHost {
    pub fn events(&self) -> &[HostEvent] {
        &self.events
    }
}

#[cfg(any(test, not(target_os = "macos")))]
impl CefHost for MockCefHost {
    type Error = HostError;

    fn create_window(&mut self, title: &str, size: WindowSize) -> Result<WindowId, Self::Error> {
        self.next_window_id += 1;
        let window_id = WindowId(self.next_window_id);
        self.events.push(HostEvent::WindowCreated {
            window_id,
            title: title.to_owned(),
            size,
        });
        Ok(window_id)
    }

    fn create_ui_view(&mut self, window_id: WindowId, url: &str) -> Result<UiViewId, Self::Error> {
        if !is_exact_ui_url(url) {
            return Err(HostError::InvalidUiUrl(url.to_owned()));
        }

        self.next_ui_view_id += 1;
        let view_id = UiViewId(self.next_ui_view_id);
        self.events.push(HostEvent::UiViewCreated {
            window_id,
            view_id,
            url: url.to_owned(),
        });
        Ok(view_id)
    }

    fn create_content_view(
        &mut self,
        window_id: WindowId,
        profile_id: ProfileId,
        tab_id: TabId,
        generation: u64,
        url: &str,
    ) -> Result<ContentViewId, Self::Error> {
        self.next_content_view_id += 1;
        let view_id = ContentViewId(self.next_content_view_id);
        self.events.push(HostEvent::ContentViewCreated {
            window_id,
            view_id,
            profile_id,
            tab_id,
            generation,
            url: url.to_owned(),
        });
        Ok(view_id)
    }

    fn navigate_content_view(
        &mut self,
        view_id: ContentViewId,
        tab_id: TabId,
        url: &str,
    ) -> Result<(), Self::Error> {
        self.events.push(HostEvent::ContentNavigated {
            view_id,
            tab_id,
            url: url.to_owned(),
        });
        Ok(())
    }

    fn set_content_view_visible(
        &mut self,
        _view_id: ContentViewId,
        _visible: bool,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn layout_content_views(
        &mut self,
        primary: Option<ContentViewId>,
        secondary: Option<ContentViewId>,
        ratio: f64,
        split_enabled: bool,
    ) -> Result<(), Self::Error> {
        self.events.push(HostEvent::ContentLayoutChanged {
            primary,
            secondary,
            ratio_millis: (ratio.clamp(0.25, 0.75) * 1_000.0).round() as u16,
            split_enabled,
        });
        Ok(())
    }

    fn set_focus_mode(&mut self, active: bool) -> Result<(), Self::Error> {
        self.events.push(HostEvent::FocusModeChanged { active });
        Ok(())
    }

    fn go_back(&mut self, view_id: ContentViewId) -> Result<(), Self::Error> {
        self.events.push(HostEvent::ContentHistoryAction {
            view_id,
            action: "back",
        });
        Ok(())
    }

    fn go_forward(&mut self, view_id: ContentViewId) -> Result<(), Self::Error> {
        self.events.push(HostEvent::ContentHistoryAction {
            view_id,
            action: "forward",
        });
        Ok(())
    }

    fn reload(&mut self, view_id: ContentViewId) -> Result<(), Self::Error> {
        self.events.push(HostEvent::ContentHistoryAction {
            view_id,
            action: "reload",
        });
        Ok(())
    }

    fn send_ui_message(&mut self, payload: &str) -> Result<(), Self::Error> {
        self.events.push(HostEvent::UiMessageSent {
            payload: payload.to_owned(),
        });
        Ok(())
    }

    fn toggle_dev_tools_for_active_content(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn clear_content_view(&mut self, _view_id: ContentViewId) -> Result<(), Self::Error> {
        Ok(())
    }

    fn destroy_content_view(&mut self, view_id: ContentViewId) -> Result<(), Self::Error> {
        self.events
            .push(HostEvent::ContentViewDestroyed { view_id });
        Ok(())
    }

    fn run_event_loop(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub type DefaultHost = NativeMacHost;
#[cfg(not(target_os = "macos"))]
pub type DefaultHost = MockCefHost;

#[cfg(target_os = "macos")]
use std::ffi::{c_char, c_int, c_void, CStr, CString};
#[cfg(target_os = "macos")]
use std::mem::{size_of, zeroed};
#[cfg(target_os = "macos")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::slice;

#[cfg(target_os = "macos")]
use switchboard_cef_sys::loader::{CefLibrary, CefSandboxContext};
#[cfg(target_os = "macos")]
use switchboard_cef_sys::raw::{
    cef_app_t, cef_base_ref_counted_t, cef_browser_host_create_browser_fn, cef_browser_settings_t,
    cef_browser_t, cef_callback_t, cef_client_t, cef_dictionary_value_t, cef_display_handler_t,
    cef_errorcode_t, cef_frame_t, cef_keyboard_handler_t, cef_life_span_handler_t,
    cef_load_handler_t, cef_main_args_t, cef_popup_features_t, cef_rect_t,
    cef_request_context_settings_t, cef_request_context_t, cef_request_t, cef_resource_handler_t,
    cef_response_t, cef_scheme_handler_factory_t, cef_scheme_registrar_t, cef_settings_t,
    cef_string_t, cef_string_utf16_t, cef_transition_type_t, cef_window_info_t,
    cef_window_open_disposition_t, CEF_RUNTIME_STYLE_ALLOY, CEF_SCHEME_OPTION_CORS_ENABLED,
    CEF_SCHEME_OPTION_DISPLAY_ISOLATED, CEF_SCHEME_OPTION_FETCH_ENABLED, CEF_SCHEME_OPTION_SECURE,
    CEF_SCHEME_OPTION_STANDARD,
};

#[cfg(target_os = "macos")]
type ObjcId = *mut c_void;
#[cfg(target_os = "macos")]
type ObjcSel = *mut c_void;

#[cfg(target_os = "macos")]
const NIL: ObjcId = std::ptr::null_mut();
#[cfg(target_os = "macos")]
const YES: i8 = 1;
#[cfg(target_os = "macos")]
const NO: i8 = 0;

const DEFAULT_WINDOW_WIDTH_PX: u32 = 1280;
const DEFAULT_WINDOW_HEIGHT_PX: u32 = 840;

fn is_exact_ui_url(url: &str) -> bool {
    url.strip_prefix("app://ui").is_some_and(|suffix| {
        suffix.is_empty()
            || suffix.starts_with('/')
            || suffix.starts_with('?')
            || suffix.starts_with('#')
    })
}

#[cfg(target_os = "macos")]
const UI_TOP_HEIGHT: f64 = 44.0;
#[cfg(target_os = "macos")]
const UI_LEFT_WIDTH: f64 = 320.0;
#[cfg(target_os = "macos")]
const DEFAULT_BACKGROUND_COLOR: u32 = 0xFF_FF_FF_FF;
#[cfg(target_os = "macos")]
const ENV_CEF_DIST: &str = "SWITCHBOARD_CEF_DIST";
#[cfg(target_os = "macos")]
const ENV_CEF_LIBRARY: &str = "SWITCHBOARD_CEF_LIBRARY";
#[cfg(target_os = "macos")]
const ENV_CEF_FRAMEWORK_DIR: &str = "SWITCHBOARD_CEF_FRAMEWORK_DIR";
#[cfg(target_os = "macos")]
const ENV_CEF_RESOURCES_DIR: &str = "SWITCHBOARD_CEF_RESOURCES_DIR";
#[cfg(target_os = "macos")]
const ENV_CEF_BROWSER_SUBPROCESS: &str = "SWITCHBOARD_CEF_BROWSER_SUBPROCESS";
#[cfg(target_os = "macos")]
const ENV_CEF_MAIN_BUNDLE_PATH: &str = "SWITCHBOARD_CEF_MAIN_BUNDLE_PATH";
#[cfg(target_os = "macos")]
const ENV_CEF_VERBOSE_ERRORS: &str = "SWITCHBOARD_CEF_VERBOSE_ERRORS";
#[cfg(target_os = "macos")]
const ENV_CEF_ROOT_CACHE_PATH: &str = "SWITCHBOARD_CEF_ROOT_CACHE_PATH";
#[cfg(target_os = "macos")]
const ENV_CEF_TMPDIR: &str = "SWITCHBOARD_CEF_TMPDIR";
#[cfg(target_os = "macos")]
const ENV_CEF_API_VERSION: &str = "SWITCHBOARD_CEF_API_VERSION";
#[cfg(target_os = "macos")]
const ENV_CEF_USE_MOCK_KEYCHAIN: &str = "SWITCHBOARD_CEF_USE_MOCK_KEYCHAIN";
#[cfg(target_os = "macos")]
const ENV_CEF_PASSWORD_STORE: &str = "SWITCHBOARD_CEF_PASSWORD_STORE";
#[cfg(target_os = "macos")]
const ENV_CEF_AUTOPLAY_POLICY: &str = "SWITCHBOARD_CEF_AUTOPLAY_POLICY";
#[cfg(target_os = "macos")]
const DEFAULT_CEF_AUTOPLAY_POLICY: &str = "no-user-gesture-required";
#[cfg(target_os = "macos")]
const DEFAULT_CEF_API_VERSION: i32 = 14500;
#[cfg(target_os = "macos")]
const STYLE_TITLED: u64 = 1 << 0;
#[cfg(target_os = "macos")]
const STYLE_CLOSABLE: u64 = 1 << 1;
#[cfg(target_os = "macos")]
const STYLE_MINIATURIZABLE: u64 = 1 << 2;
#[cfg(target_os = "macos")]
const STYLE_RESIZABLE: u64 = 1 << 3;
#[cfg(target_os = "macos")]
const STYLE_FULL_SIZE_CONTENT_VIEW: u64 = 1 << 15;
#[cfg(target_os = "macos")]
const BACKING_STORE_BUFFERED: u64 = 2;
#[cfg(target_os = "macos")]
const APP_ACTIVATION_POLICY_REGULAR: i64 = 0;
#[cfg(target_os = "macos")]
const WINDOW_TITLE_HIDDEN: i64 = 1;
#[cfg(target_os = "macos")]
const NS_VIEW_WIDTH_SIZABLE: u64 = 2;
#[cfg(target_os = "macos")]
const NS_VIEW_HEIGHT_SIZABLE: u64 = 16;
#[cfg(target_os = "macos")]
const UI_VIEW_AUTORE_SIZE_MASK: u64 = NS_VIEW_WIDTH_SIZABLE | NS_VIEW_HEIGHT_SIZABLE;
#[cfg(target_os = "macos")]
const CONTENT_VIEW_AUTORE_SIZE_MASK: u64 = NS_VIEW_WIDTH_SIZABLE | NS_VIEW_HEIGHT_SIZABLE;
#[cfg(target_os = "macos")]
const UI_SCHEME: &str = "app";
#[cfg(target_os = "macos")]
#[cfg(target_os = "macos")]
const UI_SCHEME_OPTIONS: u32 = CEF_SCHEME_OPTION_STANDARD
    | CEF_SCHEME_OPTION_SECURE
    | CEF_SCHEME_OPTION_CORS_ENABLED
    | CEF_SCHEME_OPTION_FETCH_ENABLED
    | CEF_SCHEME_OPTION_DISPLAY_ISOLATED;
#[cfg(target_os = "macos")]
const UI_SHELL_TEMPLATE_HTML: &str = include_str!("ui_shell.html");
#[cfg(target_os = "macos")]
const UI_SHELL_CSS: &str = include_str!("ui_shell.css");
#[cfg(target_os = "macos")]
const UI_SHELL_JS: &str = include_str!("ui_shell.js");
#[cfg(target_os = "macos")]
static UI_SHELL_BODY_BYTES: OnceLock<Vec<u8>> = OnceLock::new();
#[cfg(target_os = "macos")]
const WEBSITE_KEYBINDINGS_BORDER_WIDTH_PX: f64 = 2.0;
#[cfg(target_os = "macos")]
const WEBSITE_KEYBINDINGS_BORDER_RADIUS_PX: f64 = 10.0;
#[cfg(target_os = "macos")]
const CEF_KEYEVENT_RAWKEYDOWN: c_int = 0;
#[cfg(target_os = "macos")]
const CEF_KEYEVENT_KEYDOWN: c_int = 1;
#[cfg(target_os = "macos")]
const CEF_EVENTFLAG_SHIFT_DOWN: u32 = 1 << 1;
#[cfg(target_os = "macos")]
const CEF_EVENTFLAG_CONTROL_DOWN: u32 = 1 << 2;
#[cfg(target_os = "macos")]
const CEF_EVENTFLAG_ALT_DOWN: u32 = 1 << 3;
#[cfg(target_os = "macos")]
const CEF_EVENTFLAG_COMMAND_DOWN: u32 = 1 << 7;
#[cfg(target_os = "macos")]
const CEF_EVENTFLAG_IS_REPEAT: u32 = 1 << 13;
#[cfg(target_os = "macos")]
const VK_BACK: c_int = 0x08;
#[cfg(target_os = "macos")]
const VK_TAB: c_int = 0x09;
#[cfg(target_os = "macos")]
const VK_RETURN: c_int = 0x0D;
#[cfg(target_os = "macos")]
const VK_ESCAPE: c_int = 0x1B;
#[cfg(target_os = "macos")]
const VK_SPACE: c_int = 0x20;
#[cfg(target_os = "macos")]
const VK_PRIOR: c_int = 0x21;
#[cfg(target_os = "macos")]
const VK_NEXT: c_int = 0x22;
#[cfg(target_os = "macos")]
const VK_END: c_int = 0x23;
#[cfg(target_os = "macos")]
const VK_HOME: c_int = 0x24;
#[cfg(target_os = "macos")]
const VK_LEFT: c_int = 0x25;
#[cfg(target_os = "macos")]
const VK_UP: c_int = 0x26;
#[cfg(target_os = "macos")]
const VK_RIGHT: c_int = 0x27;
#[cfg(target_os = "macos")]
const VK_DOWN: c_int = 0x28;
#[cfg(target_os = "macos")]
const VK_DELETE: c_int = 0x2E;
#[cfg(target_os = "macos")]
const VK_0: c_int = 0x30;
#[cfg(target_os = "macos")]
const VK_9: c_int = 0x39;
#[cfg(target_os = "macos")]
const VK_A: c_int = 0x41;
#[cfg(target_os = "macos")]
const VK_Z: c_int = 0x5A;
#[cfg(target_os = "macos")]
const WOD_NEW_FOREGROUND_TAB: cef_window_open_disposition_t = 3;
#[cfg(target_os = "macos")]
const WOD_NEW_BACKGROUND_TAB: cef_window_open_disposition_t = 4;
#[cfg(target_os = "macos")]
const WOD_NEW_POPUP: cef_window_open_disposition_t = 5;
#[cfg(target_os = "macos")]
const WOD_NEW_WINDOW: cef_window_open_disposition_t = 6;

#[cfg(target_os = "macos")]
type CefKeyEventTypeSwitchboard = c_int;
#[cfg(target_os = "macos")]
type CefEventHandleSwitchboard = *mut c_void;

#[cfg(target_os = "macos")]
#[repr(C)]
struct cef_key_event_t_switchboard {
    type_: CefKeyEventTypeSwitchboard,
    modifiers: u32,
    windows_key_code: c_int,
    native_key_code: c_int,
    is_system_key: c_int,
    character: u16,
    unmodified_character: u16,
    focus_on_editable_field: c_int,
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct cef_keyboard_handler_t_switchboard {
    base: cef_base_ref_counted_t,
    on_pre_key_event: Option<
        unsafe extern "C" fn(
            self_: *mut cef_keyboard_handler_t,
            browser: *mut cef_browser_t,
            event: *const cef_key_event_t_switchboard,
            os_event: CefEventHandleSwitchboard,
            is_keyboard_shortcut: *mut c_int,
        ) -> c_int,
    >,
    on_key_event: Option<
        unsafe extern "C" fn(
            self_: *mut cef_keyboard_handler_t,
            browser: *mut cef_browser_t,
            event: *const cef_key_event_t_switchboard,
            os_event: CefEventHandleSwitchboard,
        ) -> c_int,
    >,
}

#[cfg(target_os = "macos")]
fn ui_shell_body() -> &'static [u8] {
    UI_SHELL_BODY_BYTES
        .get_or_init(|| {
            UI_SHELL_TEMPLATE_HTML
                .replace("/* __SWITCHBOARD_UI_SHELL_CSS__ */", UI_SHELL_CSS)
                .replace("// __SWITCHBOARD_UI_SHELL_JS__", UI_SHELL_JS)
                .into_bytes()
        })
        .as_slice()
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct NSPoint {
    x: f64,
    y: f64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct NSSize {
    width: f64,
    height: f64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct NSRect {
    origin: NSPoint,
    size: NSSize,
}

#[cfg(target_os = "macos")]
#[link(name = "objc")]
extern "C" {
    fn objc_getClass(name: *const c_char) -> ObjcId;
    fn sel_registerName(name: *const c_char) -> ObjcSel;
    fn objc_msgSend();
    fn class_addMethod(cls: ObjcId, name: ObjcSel, imp: *const c_void, types: *const c_char) -> i8;
}

#[cfg(target_os = "macos")]
#[link(name = "AppKit", kind = "framework")]
extern "C" {
    fn NSApplicationLoad() -> i8;
}

#[cfg(target_os = "macos")]
#[link(name = "WebKit", kind = "framework")]
extern "C" {}

#[cfg(target_os = "macos")]
#[link(name = "System")]
unsafe extern "C" {
    static _dispatch_main_q: u8;
    fn dispatch_async_f(
        queue: *mut c_void,
        context: *mut c_void,
        work: unsafe extern "C" fn(*mut c_void),
    );
}

#[cfg(target_os = "macos")]
static NSAPP_HANDLING_SEND_EVENT: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
static CEF_QUIT_MESSAGE_LOOP_FN: AtomicUsize = AtomicUsize::new(0);
#[cfg(target_os = "macos")]
static CEF_OPEN_BROWSER_COUNT: AtomicUsize = AtomicUsize::new(0);
#[cfg(target_os = "macos")]
static CEF_CLOSE_ALL_REQUESTED: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
static UI_SCHEME_DECLARED: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy)]
enum ContentBackend {
    WebKit(ObjcId),
    Cef(ObjcId),
}

#[cfg(target_os = "macos")]
struct CefRuntime {
    library: CefLibrary,
    config: CefConfig,
    _app: *mut cef_app_t,
    _ui_scheme_factory: *mut cef_scheme_handler_factory_t,
    ui_client: *mut cef_client_t,
    ui_request_context: *mut cef_request_context_t,
    profile_request_contexts: RefCell<HashMap<ProfileId, *mut cef_request_context_t>>,
}

#[cfg(target_os = "macos")]
struct CefConfig {
    library_path: PathBuf,
    framework_dir_path: PathBuf,
    resources_dir_path: Option<PathBuf>,
    browser_subprocess_path: Option<PathBuf>,
    main_bundle_path: Option<PathBuf>,
    root_cache_path: PathBuf,
    cache_path: PathBuf,
    temp_dir: PathBuf,
}

#[cfg(target_os = "macos")]
struct CefString {
    value: cef_string_t,
    clear: switchboard_cef_sys::raw::cef_string_utf16_clear_fn,
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct SwitchboardCefApp {
    app: cef_app_t,
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct SwitchboardUiSchemeFactory {
    factory: cef_scheme_handler_factory_t,
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct SwitchboardUiResourceHandler {
    handler: cef_resource_handler_t,
    offset: usize,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[repr(C)]
struct SwitchboardUiDisplayHandler {
    handler: cef_display_handler_t,
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct SwitchboardUiClient {
    client: cef_client_t,
    display_handler: *mut cef_display_handler_t,
    life_span_handler: *mut cef_life_span_handler_t,
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct SwitchboardContentDisplayHandler {
    handler: cef_display_handler_t,
    tab_id: TabId,
    generation: u64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct SwitchboardContentKeyboardHandler {
    handler: cef_keyboard_handler_t_switchboard,
    tab_id: TabId,
    generation: u64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct SwitchboardContentLoadHandler {
    handler: cef_load_handler_t,
    tab_id: TabId,
    generation: u64,
    main_frame_failed: AtomicBool,
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct SwitchboardContentClient {
    client: cef_client_t,
    profile_id: ProfileId,
    tab_id: TabId,
    generation: u64,
    display_handler: *mut cef_display_handler_t,
    keyboard_handler: *mut cef_keyboard_handler_t,
    load_handler: *mut cef_load_handler_t,
    life_span_handler: *mut cef_life_span_handler_t,
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct SwitchboardLifeSpanHandler {
    handler: cef_life_span_handler_t,
    tab_id: Option<TabId>,
    generation: u64,
    before_close_seen: AtomicBool,
}

#[cfg(target_os = "macos")]
impl CefConfig {
    fn from_env() -> Result<Option<Self>, HostError> {
        if let Some(library_path) = env_path(ENV_CEF_LIBRARY) {
            return Ok(Some(Self::from_library_path(library_path)?));
        }

        if let Some(dist_root) = env_path(ENV_CEF_DIST) {
            let release_dir = dist_root.join("Release");
            let framework_dir_path = env_path(ENV_CEF_FRAMEWORK_DIR)
                .unwrap_or_else(|| release_dir.join("Chromium Embedded Framework.framework"));
            let library_path = framework_dir_path.join("Chromium Embedded Framework");
            let resources_dir_path =
                env_path(ENV_CEF_RESOURCES_DIR).or(Some(framework_dir_path.join("Resources")));
            let browser_subprocess_path = env_path(ENV_CEF_BROWSER_SUBPROCESS)
                .or_else(|| detect_cef_helper_binary(&release_dir))
                .or(Some(default_subprocess_path()?));
            let main_bundle_path = env_path(ENV_CEF_MAIN_BUNDLE_PATH);
            let root_cache_path =
                env_path(ENV_CEF_ROOT_CACHE_PATH).unwrap_or(default_root_cache_path()?);
            let cache_path = root_cache_path.join("default");
            let temp_dir = env_path(ENV_CEF_TMPDIR).unwrap_or(default_temp_dir());
            return Ok(Some(Self {
                library_path,
                framework_dir_path,
                resources_dir_path,
                browser_subprocess_path,
                main_bundle_path,
                root_cache_path,
                cache_path,
                temp_dir,
            }));
        }

        Self::from_current_bundle()
    }

    fn from_current_bundle() -> Result<Option<Self>, HostError> {
        let executable = std::env::current_exe().map_err(|error| {
            HostError::Native(format!("failed to resolve Switchboard executable: {error}"))
        })?;
        let Some(bundle_path) = main_app_bundle_for_executable(&executable) else {
            return Ok(None);
        };
        let contents_dir = bundle_path.join("Contents");
        let frameworks = contents_dir.join("Frameworks");
        let framework_dir_path = frameworks.join("Chromium Embedded Framework.framework");
        let library_path = framework_dir_path.join("Chromium Embedded Framework");
        let helper = frameworks
            .join("Switchboard Helper.app")
            .join("Contents")
            .join("MacOS")
            .join("Switchboard Helper");
        let root_cache_path =
            env_path(ENV_CEF_ROOT_CACHE_PATH).unwrap_or(default_root_cache_path()?);
        Ok(Some(Self {
            library_path,
            framework_dir_path: framework_dir_path.clone(),
            resources_dir_path: Some(framework_dir_path.join("Resources")),
            browser_subprocess_path: Some(helper),
            main_bundle_path: Some(bundle_path),
            cache_path: root_cache_path.join("default"),
            root_cache_path,
            temp_dir: env_path(ENV_CEF_TMPDIR).unwrap_or(default_temp_dir()),
        }))
    }

    fn from_library_path(library_path: PathBuf) -> Result<Self, HostError> {
        let framework_dir_path = env_path(ENV_CEF_FRAMEWORK_DIR)
            .or_else(|| library_path.parent().map(Path::to_path_buf))
            .ok_or_else(|| {
                HostError::Native(
                    "cannot infer CEF framework directory from SWITCHBOARD_CEF_LIBRARY".to_owned(),
                )
            })?;
        let resources_dir_path =
            env_path(ENV_CEF_RESOURCES_DIR).or(Some(framework_dir_path.join("Resources")));
        let browser_subprocess_path =
            env_path(ENV_CEF_BROWSER_SUBPROCESS).or(Some(default_subprocess_path()?));
        let main_bundle_path = env_path(ENV_CEF_MAIN_BUNDLE_PATH);
        let root_cache_path =
            env_path(ENV_CEF_ROOT_CACHE_PATH).unwrap_or(default_root_cache_path()?);
        let cache_path = root_cache_path.join("default");
        let temp_dir = env_path(ENV_CEF_TMPDIR).unwrap_or(default_temp_dir());

        Ok(Self {
            library_path,
            framework_dir_path,
            resources_dir_path,
            browser_subprocess_path,
            main_bundle_path,
            root_cache_path,
            cache_path,
            temp_dir,
        })
    }

    fn summary(&self) -> String {
        format!(
            "  library           : {}\n  framework_dir     : {}\n  resources_dir     : {}\n  browser_subprocess: {}\n  main_bundle       : {}\n  root_cache_path   : {}\n  cache_path        : {}\n  tmp_dir           : {}",
            describe_path(&self.library_path),
            describe_path(&self.framework_dir_path),
            describe_optional_path(&self.resources_dir_path),
            describe_optional_path(&self.browser_subprocess_path),
            describe_optional_path(&self.main_bundle_path),
            describe_path(&self.root_cache_path),
            describe_path(&self.cache_path),
            describe_path(&self.temp_dir),
        )
    }

    fn validate_for_sandboxed_bundle(&self) -> Result<(), HostError> {
        let executable = std::env::current_exe().map_err(|error| {
            HostError::Native(format!("failed to resolve current executable: {error}"))
        })?;
        if !executable
            .ancestors()
            .any(|path| path.extension().and_then(|value| value.to_str()) == Some("app"))
        {
            return Err(HostError::Native(
                "CEF startup refused outside a generated macOS .app bundle".to_owned(),
            ));
        }
        for (label, path) in [
            ("CEF framework binary", Some(&self.library_path)),
            ("CEF resources", self.resources_dir_path.as_ref()),
            (
                "CEF helper executable",
                self.browser_subprocess_path.as_ref(),
            ),
        ] {
            let Some(path) = path else {
                return Err(HostError::Native(format!("{label} is not configured")));
            };
            if !path.exists() {
                return Err(HostError::Native(format!(
                    "{label} is missing: {}",
                    path.display()
                )));
            }
        }
        let sandbox_library = self
            .framework_dir_path
            .join("Libraries/libcef_sandbox.dylib");
        if !sandbox_library.exists() {
            return Err(HostError::Native(format!(
                "CEF sandbox library is missing: {}",
                sandbox_library.display()
            )));
        }
        Ok(())
    }

    fn sandbox_library_path(&self) -> PathBuf {
        self.framework_dir_path
            .join("Libraries/libcef_sandbox.dylib")
    }
}

#[cfg(target_os = "macos")]
fn main_app_bundle_for_executable(executable: &Path) -> Option<PathBuf> {
    let containing_bundle = executable
        .ancestors()
        .find(|path| path.extension().and_then(|value| value.to_str()) == Some("app"))?;

    // CEF helpers live at:
    // Main.app/Contents/Frameworks/Main Helper.app/Contents/MacOS/Helper.
    // A helper must resolve framework/resources/main_bundle_path against the
    // outer application, not against its own nested bundle.
    let outer_bundle = containing_bundle
        .parent()
        .filter(|path| path.file_name().and_then(|value| value.to_str()) == Some("Frameworks"))
        .and_then(Path::parent)
        .filter(|path| path.file_name().and_then(|value| value.to_str()) == Some("Contents"))
        .and_then(Path::parent)
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("app"));

    Some(outer_bundle.unwrap_or(containing_bundle).to_path_buf())
}

#[cfg(target_os = "macos")]
impl CefString {
    fn new(library: &CefLibrary, value: &str) -> Result<Self, HostError> {
        let utf16: Vec<u16> = value.encode_utf16().collect();
        let mut cef_value: cef_string_utf16_t = unsafe { zeroed() };
        let ok = unsafe {
            (library.api.cef_string_utf16_set)(utf16.as_ptr(), utf16.len(), &mut cef_value, 1)
        };
        if ok == 0 {
            return Err(HostError::Native(format!(
                "failed to convert string into CEF UTF-16 buffer: {value}"
            )));
        }
        Ok(Self {
            value: cef_value,
            clear: library.api.cef_string_utf16_clear,
        })
    }

    fn value(&self) -> cef_string_t {
        self.value
    }

    fn as_ptr(&self) -> *const cef_string_t {
        &self.value as *const cef_string_t
    }
}

#[cfg(target_os = "macos")]
impl Drop for CefString {
    fn drop(&mut self) {
        unsafe {
            (self.clear)(&mut self.value);
        }
    }
}

#[cfg(target_os = "macos")]
fn cef_reference_counts() -> &'static Mutex<HashMap<usize, usize>> {
    static COUNTS: OnceLock<Mutex<HashMap<usize, usize>>> = OnceLock::new();
    COUNTS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn cef_ref_counted_add_ref(self_: *mut cef_base_ref_counted_t) {
    if self_.is_null() {
        return;
    }
    let mut counts = cef_reference_counts()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let count = counts.entry(self_ as usize).or_insert(1);
    *count = count.saturating_add(1);
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn cef_ref_counted_release(self_: *mut cef_base_ref_counted_t) -> c_int {
    if self_.is_null() {
        return 0;
    }
    let mut counts = cef_reference_counts()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let count = counts.entry(self_ as usize).or_insert(1);
    if *count <= 1 {
        counts.remove(&(self_ as usize));
        1
    } else {
        *count -= 1;
        0
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn cef_ref_counted_has_one_ref(self_: *mut cef_base_ref_counted_t) -> c_int {
    if self_.is_null() {
        return 0;
    }
    let counts = cef_reference_counts()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    i32::from(counts.get(&(self_ as usize)).copied().unwrap_or(1) == 1)
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn cef_ref_counted_has_at_least_one_ref(
    self_: *mut cef_base_ref_counted_t,
) -> c_int {
    i32::from(!self_.is_null())
}

#[cfg(target_os = "macos")]
fn ref_counted_base<T>() -> cef_base_ref_counted_t {
    cef_base_ref_counted_t {
        size: size_of::<T>(),
        add_ref: Some(cef_ref_counted_add_ref),
        release: Some(cef_ref_counted_release),
        has_one_ref: Some(cef_ref_counted_has_one_ref),
        has_at_least_one_ref: Some(cef_ref_counted_has_at_least_one_ref),
    }
}

#[cfg(target_os = "macos")]
unsafe fn cef_string_to_owned(value: *const cef_string_t) -> String {
    if value.is_null() {
        return String::new();
    }
    if (*value).str_.is_null() || (*value).length == 0 {
        return String::new();
    }
    let units = slice::from_raw_parts((*value).str_, (*value).length);
    String::from_utf16_lossy(units)
}

#[cfg(target_os = "macos")]
fn with_stack_cef_string(value: &str, callback: impl FnOnce(*const cef_string_t)) {
    let utf16: Vec<u16> = value.encode_utf16().collect();
    let cef_value = cef_string_t {
        str_: utf16.as_ptr() as *mut u16,
        length: utf16.len(),
        dtor: None,
    };
    callback(&cef_value);
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_app_on_register_custom_schemes(
    _self_: *mut cef_app_t,
    registrar: *mut cef_scheme_registrar_t,
) {
    if registrar.is_null() {
        return;
    }
    let Some(add_custom_scheme) = (*registrar).add_custom_scheme else {
        return;
    };
    with_stack_cef_string(UI_SCHEME, |scheme_name| unsafe {
        let registered = add_custom_scheme(registrar, scheme_name, UI_SCHEME_OPTIONS as c_int);
        if registered != 0 {
            UI_SCHEME_DECLARED.store(true, Ordering::Release);
            if env_flag(ENV_CEF_VERBOSE_ERRORS) {
                eprintln!(
                    "switchboard-app: declared custom app:// scheme (pid={})",
                    std::process::id()
                );
            }
        } else {
            eprintln!("switchboard-app: failed to declare custom app:// scheme");
        }
    });
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_nsapp_is_handling_send_event(_self: ObjcId, _cmd: ObjcSel) -> i8 {
    if NSAPP_HANDLING_SEND_EVENT.load(Ordering::Relaxed) {
        YES
    } else {
        NO
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_nsapp_set_handling_send_event(
    _self: ObjcId,
    _cmd: ObjcSel,
    value: i8,
) {
    NSAPP_HANDLING_SEND_EVENT.store(value != NO, Ordering::Relaxed);
}

#[cfg(target_os = "macos")]
fn install_nsapplication_event_shim() -> Result<(), HostError> {
    unsafe {
        let app_class = objc_class("NSApplication")?;
        let is_handling_selector = selector("isHandlingSendEvent")?;
        let set_handling_selector = selector("setHandlingSendEvent:")?;
        let is_encoding = CString::new("c@:").expect("static signature should be valid");
        let set_encoding = CString::new("v@:c").expect("static signature should be valid");

        let _ = class_addMethod(
            app_class,
            is_handling_selector,
            switchboard_nsapp_is_handling_send_event as *const c_void,
            is_encoding.as_ptr(),
        );
        let _ = class_addMethod(
            app_class,
            set_handling_selector,
            switchboard_nsapp_set_handling_send_event as *const c_void,
            set_encoding.as_ptr(),
        );
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn install_cef_quit_message_loop_hook(quit: unsafe extern "C" fn()) {
    CEF_QUIT_MESSAGE_LOOP_FN.store(quit as usize, Ordering::Release);
}

#[cfg(target_os = "macos")]
fn clear_cef_quit_message_loop_hook() {
    CEF_QUIT_MESSAGE_LOOP_FN.store(0, Ordering::Release);
}

#[cfg(target_os = "macos")]
fn quit_cef_message_loop_if_available() {
    let raw = CEF_QUIT_MESSAGE_LOOP_FN.load(Ordering::Acquire);
    if raw == 0 {
        return;
    }
    let quit: unsafe extern "C" fn() = unsafe { std::mem::transmute(raw) };
    unsafe {
        quit();
    }
}

#[cfg(target_os = "macos")]
unsafe fn request_cef_browser_close(browser: *mut cef_browser_t) {
    if browser.is_null() {
        return;
    }
    let Some(get_host) = (*browser).get_host else {
        return;
    };
    let host = get_host(browser);
    if host.is_null() {
        return;
    }
    if let Some(close_browser) = (*host).close_browser {
        close_browser(host, 1);
    }
}

#[cfg(target_os = "macos")]
fn request_all_cef_browsers_close() {
    CEF_CLOSE_ALL_REQUESTED.store(true, Ordering::Release);
    let mut browsers = Vec::new();
    let ui_browser = ui_shell_browser();
    if !ui_browser.is_null() {
        browsers.push(ui_browser);
    }
    CONTENT_BROWSERS_BY_TAB.with(|slot| {
        for browser in slot.borrow().values().map(|entry| entry.browser) {
            if !browser.is_null() && !browsers.contains(&browser) {
                browsers.push(browser);
            }
        }
    });

    if browsers.is_empty() {
        quit_cef_message_loop_if_available();
        return;
    }
    for browser in browsers {
        unsafe {
            request_cef_browser_close(browser);
        }
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_nsapp_window_should_close(
    self_: ObjcId,
    _cmd: ObjcSel,
    _window: ObjcId,
) -> i8 {
    let _ = self_;
    request_all_cef_browsers_close();
    NO
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_nsapp_application_will_terminate(
    _self: ObjcId,
    _cmd: ObjcSel,
    _notification: ObjcId,
) {
    request_all_cef_browsers_close();
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_nsapp_application_did_become_active(
    _self: ObjcId,
    _cmd: ObjcSel,
    _notification: ObjcId,
) {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64;
    emit_ui_command(UiCommand::WakeSnoozed { now_ms });
}

#[cfg(target_os = "macos")]
unsafe fn emit_window_size_from_notification(notification: ObjcId, skip_live_resize: bool) {
    if notification == NIL {
        return;
    }

    let object_sel = sel_registerName(b"object\0".as_ptr() as *const c_char);
    if object_sel == NIL {
        return;
    }
    let window = msg_send_id(notification, object_sel);
    if window == NIL {
        return;
    }

    if skip_live_resize {
        let in_live_resize_sel = sel_registerName(b"inLiveResize\0".as_ptr() as *const c_char);
        if in_live_resize_sel != NIL && msg_send_bool(window, in_live_resize_sel) != NO {
            return;
        }
    }

    let frame_sel = sel_registerName(b"frame\0".as_ptr() as *const c_char);
    if frame_sel == NIL {
        return;
    }
    let frame = msg_send_rect(window, frame_sel);
    let width = frame.size.width.round().max(1.0) as u32;
    let height = frame.size.height.round().max(1.0) as u32;
    emit_window_event(WindowEvent::Resized { width, height });
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_nsapp_window_did_resize(
    _self: ObjcId,
    _cmd: ObjcSel,
    notification: ObjcId,
) {
    emit_window_size_from_notification(notification, true);
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_nsapp_window_did_end_live_resize(
    _self: ObjcId,
    _cmd: ObjcSel,
    notification: ObjcId,
) {
    emit_window_size_from_notification(notification, false);
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_nsapp_should_handle_reopen(
    self_: ObjcId,
    _cmd: ObjcSel,
    _application: ObjcId,
    has_visible_windows: i8,
) -> i8 {
    if has_visible_windows == NO {
        let windows_sel = sel_registerName(b"windows\0".as_ptr() as *const c_char);
        let count_sel = sel_registerName(b"count\0".as_ptr() as *const c_char);
        let object_at_index_sel = sel_registerName(b"objectAtIndex:\0".as_ptr() as *const c_char);
        let make_key_and_order_front_sel =
            sel_registerName(b"makeKeyAndOrderFront:\0".as_ptr() as *const c_char);
        if windows_sel != NIL
            && count_sel != NIL
            && object_at_index_sel != NIL
            && make_key_and_order_front_sel != NIL
        {
            let windows = msg_send_id(self_, windows_sel);
            if windows != NIL && msg_send_usize(windows, count_sel) > 0 {
                let window = msg_send_id_usize(windows, object_at_index_sel, 0);
                if window != NIL {
                    msg_send_void_id(window, make_key_and_order_front_sel, NIL);
                }
            }
        }
    }
    let activate_sel = sel_registerName(b"activateIgnoringOtherApps:\0".as_ptr() as *const c_char);
    if activate_sel != NIL {
        msg_send_void_bool(self_, activate_sel, YES);
    }
    YES
}

#[cfg(target_os = "macos")]
fn install_nsapplication_suspend_shim() -> Result<(), HostError> {
    unsafe {
        let app_class = objc_class("NSApplication")?;
        let window_should_close_selector = selector("windowShouldClose:")?;
        let window_did_resize_selector = selector("windowDidResize:")?;
        let window_did_end_live_resize_selector = selector("windowDidEndLiveResize:")?;
        let should_handle_reopen_selector =
            selector("applicationShouldHandleReopen:hasVisibleWindows:")?;
        let will_terminate_selector = selector("applicationWillTerminate:")?;
        let did_become_active_selector = selector("applicationDidBecomeActive:")?;
        let window_close_encoding = CString::new("c@:@").expect("static signature should be valid");
        let window_resize_encoding =
            CString::new("v@:@").expect("static signature should be valid");
        let reopen_encoding = CString::new("c@:@c").expect("static signature should be valid");
        let will_terminate_encoding =
            CString::new("v@:@").expect("static signature should be valid");
        let did_become_active_encoding =
            CString::new("v@:@").expect("static signature should be valid");

        let _ = class_addMethod(
            app_class,
            window_should_close_selector,
            switchboard_nsapp_window_should_close as *const c_void,
            window_close_encoding.as_ptr(),
        );
        let _ = class_addMethod(
            app_class,
            window_did_resize_selector,
            switchboard_nsapp_window_did_resize as *const c_void,
            window_resize_encoding.as_ptr(),
        );
        let _ = class_addMethod(
            app_class,
            window_did_end_live_resize_selector,
            switchboard_nsapp_window_did_end_live_resize as *const c_void,
            window_resize_encoding.as_ptr(),
        );
        let _ = class_addMethod(
            app_class,
            should_handle_reopen_selector,
            switchboard_nsapp_should_handle_reopen as *const c_void,
            reopen_encoding.as_ptr(),
        );
        let _ = class_addMethod(
            app_class,
            will_terminate_selector,
            switchboard_nsapp_application_will_terminate as *const c_void,
            will_terminate_encoding.as_ptr(),
        );
        let _ = class_addMethod(
            app_class,
            did_become_active_selector,
            switchboard_nsapp_application_did_become_active as *const c_void,
            did_become_active_encoding.as_ptr(),
        );
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn allocate_cef_app() -> *mut cef_app_t {
    let app = Box::new(SwitchboardCefApp {
        app: cef_app_t {
            base: ref_counted_base::<cef_app_t>(),
            on_before_command_line_processing: None,
            on_register_custom_schemes: Some(switchboard_app_on_register_custom_schemes),
            get_resource_bundle_handler: None,
            get_browser_process_handler: None,
            get_render_process_handler: None,
        },
    });
    let app_ptr = Box::into_raw(app);
    unsafe { &mut (*app_ptr).app as *mut cef_app_t }
}

#[cfg(target_os = "macos")]
fn decode_uri_component(value: &str) -> Result<String, &'static str> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let high = (bytes[index + 1] as char)
                    .to_digit(16)
                    .ok_or("invalid percent escape")?;
                let low = (bytes[index + 2] as char)
                    .to_digit(16)
                    .ok_or("invalid percent escape")?;
                decoded.push(((high << 4) | low) as u8);
                index += 3;
            }
            b'%' => return Err("truncated percent escape"),
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).map_err(|_| "bridge payload is not UTF-8")
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_ui_on_address_change(
    _self_: *mut cef_display_handler_t,
    browser: *mut cef_browser_t,
    frame: *mut cef_frame_t,
    url: *const cef_string_t,
) {
    if browser.is_null() || frame.is_null() {
        return;
    }
    if !(*frame).is_main.is_some_and(|is_main| is_main(frame) != 0) {
        return;
    }
    let url = cef_string_to_owned(url);
    if !is_exact_ui_url(&url) {
        return;
    }
    if !ui_shell_browser_matches(browser) {
        eprintln!("switchboard-app: rejected bridge callback from a non-UI browser");
        return;
    }
    // CEF may expose a different C wrapper for the same underlying browser in
    // each callback. Retain the wrapper from this callback before dispatching
    // the intent because the synchronous response path immediately accesses
    // the UI browser again.
    set_ui_shell_browser(browser);
    let Some((_, encoded)) = url.split_once("#bridge=") else {
        return;
    };
    let payload = match decode_uri_component(encoded) {
        Ok(payload) => payload,
        Err(reason) => {
            eprintln!("switchboard-app: rejected UI bridge payload ({reason})");
            return;
        }
    };
    match decode_client_message(&payload) {
        Ok(envelope) => match envelope.command {
            UiCommand::SetUiOverlay { visible } => {
                if let Err(error) = set_ui_overlay_visible(visible) {
                    eprintln!("switchboard-app: failed to update UI overlay: {error}");
                }
            }
            UiCommand::SetBrowserMode { active } => {
                if let Err(error) = set_website_keybindings_active(active) {
                    eprintln!("switchboard-app: failed to update browser keyboard mode: {error}");
                }
            }
            UiCommand::SyncKeybindings {
                close_tab,
                command_palette,
                focus_navigation,
                toggle_devtools,
            } => set_browser_shortcut_bindings(
                &close_tab,
                &command_palette,
                &focus_navigation,
                &toggle_devtools,
            ),
            // Browser creation is illegal while CEF is still inside this
            // display callback. Run all stateful bridge intents on the next
            // main-queue turn so the callback has fully unwound first.
            command => defer_ui_command(command),
        },
        Err(error) => eprintln!("switchboard-app: rejected UI bridge payload: {error:?}"),
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_ui_client_get_display_handler(
    self_: *mut cef_client_t,
) -> *mut cef_display_handler_t {
    if self_.is_null() {
        return std::ptr::null_mut();
    }
    let client = self_ as *mut SwitchboardUiClient;
    (*client).display_handler
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_ui_client_get_life_span_handler(
    self_: *mut cef_client_t,
) -> *mut cef_life_span_handler_t {
    if self_.is_null() {
        return std::ptr::null_mut();
    }
    let client = self_ as *mut SwitchboardUiClient;
    (*client).life_span_handler
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_content_client_get_life_span_handler(
    self_: *mut cef_client_t,
) -> *mut cef_life_span_handler_t {
    if self_.is_null() {
        return std::ptr::null_mut();
    }
    let client = self_ as *mut SwitchboardContentClient;
    (*client).life_span_handler
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_content_before_popup(
    self_: *mut cef_life_span_handler_t,
    _browser: *mut cef_browser_t,
    _frame: *mut cef_frame_t,
    _popup_id: c_int,
    target_url: *const cef_string_t,
    _target_frame_name: *const cef_string_t,
    target_disposition: cef_window_open_disposition_t,
    user_gesture: c_int,
    _popup_features: *const cef_popup_features_t,
    _window_info: *mut cef_window_info_t,
    _client: *mut *mut cef_client_t,
    _settings: *mut cef_browser_settings_t,
    _extra_info: *mut *mut cef_dictionary_value_t,
    no_javascript_access: *mut c_int,
) -> c_int {
    if !no_javascript_access.is_null() {
        *no_javascript_access = 1;
    }
    if self_.is_null() {
        return 1;
    }
    let handler = self_ as *mut SwitchboardLifeSpanHandler;
    let Some(source_tab_id) = (*handler).tab_id else {
        return 1;
    };
    let url = cef_string_to_owned(target_url);
    if user_gesture == 0 || url.is_empty() || url.starts_with("app://") {
        return 1;
    }

    let supported_disposition = matches!(
        target_disposition,
        WOD_NEW_FOREGROUND_TAB | WOD_NEW_BACKGROUND_TAB | WOD_NEW_POPUP | WOD_NEW_WINDOW
    );
    if supported_disposition {
        emit_ui_command(UiCommand::OpenLinkFromTab {
            source_tab_id: source_tab_id.0,
            generation: (*handler).generation,
            url,
            secondary: target_disposition == WOD_NEW_WINDOW,
        });
    }
    // Switchboard owns tab and split creation. Never allow CEF to create an
    // unmanaged native popup/Chrome window.
    1
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_browser_after_created(
    self_: *mut cef_life_span_handler_t,
    browser: *mut cef_browser_t,
) {
    if self_.is_null() || browser.is_null() {
        return;
    }
    let handler = self_ as *mut SwitchboardLifeSpanHandler;
    (*handler).before_close_seen.store(false, Ordering::Release);
    CEF_OPEN_BROWSER_COUNT.fetch_add(1, Ordering::AcqRel);
    if let Some(tab_id) = (*handler).tab_id {
        remember_browser_for_tab(tab_id, (*handler).generation, browser);
    } else {
        remember_ui_shell_browser(browser);
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_browser_before_close(
    self_: *mut cef_life_span_handler_t,
    browser: *mut cef_browser_t,
) {
    if self_.is_null() {
        return;
    }
    let handler = self_ as *mut SwitchboardLifeSpanHandler;
    if (*handler).before_close_seen.swap(true, Ordering::AcqRel) {
        return;
    }

    if let Some(tab_id) = (*handler).tab_id {
        let is_current_browser = content_browser_matches(tab_id, (*handler).generation, browser);
        if active_content_tab() == Some(tab_id) && is_current_browser {
            set_active_content_browser(std::ptr::null_mut());
        }
        if is_current_browser {
            // Drop Switchboard's retained reference only after all routing
            // checks for this OnBeforeClose callback are complete.
            forget_browser_for_tab(tab_id, (*handler).generation);
        }
        emit_content_event(ContentEvent::BrowserClosed {
            tab_id,
            generation: (*handler).generation,
        });
    } else if ui_shell_browser_matches(browser) {
        set_ui_shell_browser(std::ptr::null_mut());
    }

    let previous = CEF_OPEN_BROWSER_COUNT
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
            Some(count.saturating_sub(1))
        })
        .unwrap_or(0);
    if previous <= 1 {
        CEF_OPEN_BROWSER_COUNT.store(0, Ordering::Release);
        if CEF_CLOSE_ALL_REQUESTED.load(Ordering::Acquire) {
            quit_cef_message_loop_if_available();
        }
    }
}

#[cfg(target_os = "macos")]
fn allocate_life_span_handler(
    tab_id: Option<TabId>,
    generation: u64,
) -> *mut cef_life_span_handler_t {
    let on_before_popup = if tab_id.is_some() {
        Some(switchboard_content_before_popup as _)
    } else {
        None
    };
    let handler = Box::new(SwitchboardLifeSpanHandler {
        handler: cef_life_span_handler_t {
            base: ref_counted_base::<cef_life_span_handler_t>(),
            on_before_popup,
            on_before_popup_aborted: None,
            on_before_dev_tools_popup: None,
            on_after_created: Some(switchboard_browser_after_created),
            do_close: None,
            on_before_close: Some(switchboard_browser_before_close),
        },
        tab_id,
        generation,
        before_close_seen: AtomicBool::new(false),
    });
    let pointer = Box::into_raw(handler);
    unsafe { &mut (*pointer).handler }
}

#[cfg(target_os = "macos")]
fn allocate_ui_display_handler() -> *mut cef_display_handler_t {
    let handler = Box::new(SwitchboardUiDisplayHandler {
        handler: cef_display_handler_t {
            base: ref_counted_base::<cef_display_handler_t>(),
            on_address_change: Some(switchboard_ui_on_address_change),
            on_title_change: None,
            on_favicon_urlchange: None,
            on_fullscreen_mode_change: None,
            on_tooltip: None,
            on_status_message: None,
            on_console_message: None,
            on_auto_resize: None,
            on_loading_progress_change: None,
            on_cursor_change: None,
            on_media_access_change: None,
            on_contents_bounds_change: None,
            get_root_window_screen_rect: None,
        },
    });
    let pointer = Box::into_raw(handler);
    unsafe { &mut (*pointer).handler }
}

#[cfg(target_os = "macos")]
fn allocate_ui_cef_client() -> *mut cef_client_t {
    let display_handler = allocate_ui_display_handler();
    let life_span_handler = allocate_life_span_handler(None, 0);
    let client = Box::new(SwitchboardUiClient {
        client: cef_client_t {
            base: ref_counted_base::<cef_client_t>(),
            get_audio_handler: None,
            get_command_handler: None,
            get_context_menu_handler: None,
            get_dialog_handler: None,
            get_display_handler: Some(switchboard_ui_client_get_display_handler),
            get_download_handler: None,
            get_drag_handler: None,
            get_find_handler: None,
            get_focus_handler: None,
            get_frame_handler: None,
            get_permission_handler: None,
            get_jsdialog_handler: None,
            get_keyboard_handler: None,
            get_life_span_handler: Some(switchboard_ui_client_get_life_span_handler),
            get_load_handler: None,
            get_print_handler: None,
            get_render_handler: None,
            get_request_handler: None,
            on_process_message_received: None,
        },
        display_handler,
        life_span_handler,
    });
    let ptr = Box::into_raw(client);
    unsafe { &mut (*ptr).client as *mut cef_client_t }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_resource_handler_open(
    _self_: *mut cef_resource_handler_t,
    _request: *mut cef_request_t,
    handle_request: *mut c_int,
    _callback: *mut cef_callback_t,
) -> c_int {
    if !handle_request.is_null() {
        *handle_request = 1;
    }
    1
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_resource_handler_process_request(
    _self_: *mut cef_resource_handler_t,
    _request: *mut cef_request_t,
    callback: *mut cef_callback_t,
) -> c_int {
    if !callback.is_null() {
        if let Some(cont) = (*callback).cont {
            cont(callback);
        }
    }
    1
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_resource_handler_get_response_headers(
    _self_: *mut cef_resource_handler_t,
    response: *mut cef_response_t,
    response_length: *mut i64,
    _redirect_url: *mut cef_string_t,
) {
    if !response_length.is_null() {
        *response_length = ui_shell_body().len() as i64;
    }
    if response.is_null() {
        return;
    }
    if let Some(set_status) = (*response).set_status {
        set_status(response, 200);
    }
    if let Some(set_status_text) = (*response).set_status_text {
        with_stack_cef_string("OK", |value| unsafe {
            set_status_text(response, value);
        });
    }
    if let Some(set_mime_type) = (*response).set_mime_type {
        with_stack_cef_string("text/html", |value| unsafe {
            set_mime_type(response, value);
        });
    }
    if let Some(set_charset) = (*response).set_charset {
        with_stack_cef_string("utf-8", |value| unsafe {
            set_charset(response, value);
        });
    }
    if let Some(set_header_by_name) = (*response).set_header_by_name {
        with_stack_cef_string("Cache-Control", |name| {
            with_stack_cef_string(
                "no-store, no-cache, must-revalidate, max-age=0",
                |value| unsafe {
                    set_header_by_name(response, name, value, 1);
                },
            );
        });
        with_stack_cef_string("Pragma", |name| {
            with_stack_cef_string("no-cache", |value| unsafe {
                set_header_by_name(response, name, value, 1);
            });
        });
        with_stack_cef_string("Expires", |name| {
            with_stack_cef_string("0", |value| unsafe {
                set_header_by_name(response, name, value, 1);
            });
        });
        with_stack_cef_string("Content-Security-Policy", |name| {
            with_stack_cef_string(
                "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'",
                |value| unsafe {
                    set_header_by_name(response, name, value, 1);
                },
            );
        });
        with_stack_cef_string("X-Content-Type-Options", |name| {
            with_stack_cef_string("nosniff", |value| unsafe {
                set_header_by_name(response, name, value, 1);
            });
        });
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_resource_handler_skip(
    self_: *mut cef_resource_handler_t,
    bytes_to_skip: i64,
    bytes_skipped: *mut i64,
    _callback: *mut switchboard_cef_sys::raw::cef_resource_skip_callback_t,
) -> c_int {
    if self_.is_null() {
        return 0;
    }
    let this = self_ as *mut SwitchboardUiResourceHandler;
    if bytes_to_skip <= 0 {
        if !bytes_skipped.is_null() {
            *bytes_skipped = 0;
        }
        return 1;
    }
    let total = ui_shell_body().len();
    let remaining = total.saturating_sub((*this).offset);
    let to_skip = (bytes_to_skip as usize).min(remaining);
    (*this).offset += to_skip;
    if !bytes_skipped.is_null() {
        *bytes_skipped = to_skip as i64;
    }
    1
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_resource_handler_read(
    self_: *mut cef_resource_handler_t,
    data_out: *mut c_void,
    bytes_to_read: c_int,
    bytes_read: *mut c_int,
    _callback: *mut switchboard_cef_sys::raw::cef_resource_read_callback_t,
) -> c_int {
    if self_.is_null() || data_out.is_null() || bytes_to_read <= 0 {
        if !bytes_read.is_null() {
            *bytes_read = 0;
        }
        return 0;
    }
    let this = self_ as *mut SwitchboardUiResourceHandler;
    let body = ui_shell_body();
    let remaining = body.len().saturating_sub((*this).offset);
    if remaining == 0 {
        if !bytes_read.is_null() {
            *bytes_read = 0;
        }
        return 0;
    }

    let chunk_len = (bytes_to_read as usize).min(remaining);
    std::ptr::copy_nonoverlapping(
        body.as_ptr().add((*this).offset),
        data_out as *mut u8,
        chunk_len,
    );
    (*this).offset += chunk_len;
    if !bytes_read.is_null() {
        *bytes_read = chunk_len as c_int;
    }
    1
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_resource_handler_read_response(
    self_: *mut cef_resource_handler_t,
    data_out: *mut c_void,
    bytes_to_read: c_int,
    bytes_read: *mut c_int,
    _callback: *mut cef_callback_t,
) -> c_int {
    switchboard_resource_handler_read(
        self_,
        data_out,
        bytes_to_read,
        bytes_read,
        std::ptr::null_mut(),
    )
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_resource_handler_cancel(_self_: *mut cef_resource_handler_t) {}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_scheme_factory_create(
    _self_: *mut cef_scheme_handler_factory_t,
    browser: *mut cef_browser_t,
    _frame: *mut cef_frame_t,
    _scheme_name: *const cef_string_t,
    _request: *mut cef_request_t,
) -> *mut cef_resource_handler_t {
    if browser.is_null() {
        return std::ptr::null_mut();
    }
    let handler = Box::new(SwitchboardUiResourceHandler {
        handler: cef_resource_handler_t {
            base: ref_counted_base::<cef_resource_handler_t>(),
            open: Some(switchboard_resource_handler_open),
            process_request: Some(switchboard_resource_handler_process_request),
            get_response_headers: Some(switchboard_resource_handler_get_response_headers),
            skip: Some(switchboard_resource_handler_skip),
            read: Some(switchboard_resource_handler_read),
            read_response: Some(switchboard_resource_handler_read_response),
            cancel: Some(switchboard_resource_handler_cancel),
        },
        offset: 0,
    });
    let ptr = Box::into_raw(handler);
    &mut (*ptr).handler as *mut cef_resource_handler_t
}

#[cfg(target_os = "macos")]
fn allocate_ui_scheme_factory() -> *mut cef_scheme_handler_factory_t {
    let factory = Box::new(SwitchboardUiSchemeFactory {
        factory: cef_scheme_handler_factory_t {
            base: ref_counted_base::<cef_scheme_handler_factory_t>(),
            create: Some(switchboard_scheme_factory_create),
        },
    });
    let ptr = Box::into_raw(factory);
    unsafe { &mut (*ptr).factory as *mut cef_scheme_handler_factory_t }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_content_on_address_change(
    self_: *mut cef_display_handler_t,
    browser: *mut cef_browser_t,
    frame: *mut cef_frame_t,
    url: *const cef_string_t,
) {
    if self_.is_null() {
        return;
    }
    let handler = self_ as *mut SwitchboardContentDisplayHandler;
    let tab_id = (*handler).tab_id;
    let generation = (*handler).generation;
    if !cef_frame_is_main(frame) {
        return;
    }
    let next = cef_string_to_owned(url);
    if next.is_empty() {
        return;
    }
    if !(next.starts_with("https://") || next.starts_with("http://")) {
        return;
    }
    if !remember_browser_for_tab(tab_id, generation, browser) {
        return;
    }
    emit_content_event(ContentEvent::UrlChanged {
        tab_id,
        generation,
        url: next.clone(),
    });
    if env_flag(ENV_CEF_VERBOSE_ERRORS) {
        eprintln!("switchboard-app: observed content URL change -> {next}");
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_content_on_title_change(
    self_: *mut cef_display_handler_t,
    browser: *mut cef_browser_t,
    title: *const cef_string_t,
) {
    if self_.is_null() {
        return;
    }
    let handler = self_ as *mut SwitchboardContentDisplayHandler;
    let tab_id = (*handler).tab_id;
    let generation = (*handler).generation;
    if !remember_browser_for_tab(tab_id, generation, browser) {
        return;
    }
    let next = cef_string_to_owned(title);
    emit_content_event(ContentEvent::TitleChanged {
        tab_id,
        generation,
        title: next,
    });
}

#[cfg(target_os = "macos")]
fn cef_frame_is_main(frame: *mut cef_frame_t) -> bool {
    if frame.is_null() {
        return false;
    }
    unsafe { (*frame).is_main.is_some_and(|is_main| is_main(frame) != 0) }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_content_on_loading_state_change(
    self_: *mut cef_load_handler_t,
    browser: *mut cef_browser_t,
    is_loading: c_int,
    can_go_back: c_int,
    can_go_forward: c_int,
) {
    if self_.is_null() {
        return;
    }
    let handler = self_ as *mut SwitchboardContentLoadHandler;
    let tab_id = (*handler).tab_id;
    let generation = (*handler).generation;
    if !remember_browser_for_tab(tab_id, generation, browser) {
        return;
    }
    emit_content_event(ContentEvent::LoadingChanged {
        tab_id,
        generation,
        is_loading: is_loading != 0,
    });
    emit_content_event(ContentEvent::NavigationStateChanged {
        tab_id,
        generation,
        can_go_back: can_go_back != 0,
        can_go_forward: can_go_forward != 0,
    });
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_content_on_load_start(
    self_: *mut cef_load_handler_t,
    browser: *mut cef_browser_t,
    frame: *mut cef_frame_t,
    _transition_type: cef_transition_type_t,
) {
    if self_.is_null() || !cef_frame_is_main(frame) {
        return;
    }
    let handler = self_ as *mut SwitchboardContentLoadHandler;
    if !remember_browser_for_tab((*handler).tab_id, (*handler).generation, browser) {
        return;
    }
    (*handler).main_frame_failed.store(false, Ordering::Release);
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_content_on_load_end(
    self_: *mut cef_load_handler_t,
    browser: *mut cef_browser_t,
    frame: *mut cef_frame_t,
    _http_status_code: c_int,
) {
    if self_.is_null() || !cef_frame_is_main(frame) {
        return;
    }
    let handler = self_ as *mut SwitchboardContentLoadHandler;
    if !remember_browser_for_tab((*handler).tab_id, (*handler).generation, browser) {
        return;
    }
    if !(*handler).main_frame_failed.swap(false, Ordering::AcqRel) {
        emit_content_event(ContentEvent::MainFrameLoadSucceeded {
            tab_id: (*handler).tab_id,
            generation: (*handler).generation,
        });
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_content_on_load_error(
    self_: *mut cef_load_handler_t,
    browser: *mut cef_browser_t,
    frame: *mut cef_frame_t,
    error_code: cef_errorcode_t,
    error_text: *const cef_string_t,
    failed_url: *const cef_string_t,
) {
    if self_.is_null() || !cef_frame_is_main(frame) {
        return;
    }
    let handler = self_ as *mut SwitchboardContentLoadHandler;
    if !remember_browser_for_tab((*handler).tab_id, (*handler).generation, browser) {
        return;
    }
    (*handler).main_frame_failed.store(true, Ordering::Release);
    let error_text = bounded_diagnostic(&cef_string_to_owned(error_text), 256);
    let failed_url = bounded_diagnostic(&cef_string_to_owned(failed_url), 512);
    eprintln!(
        "switchboard-app: main-frame load failed tab={} generation={} code={} error={} url={}",
        (*handler).tab_id.0,
        (*handler).generation,
        error_code,
        error_text,
        failed_url
    );
}

#[cfg(target_os = "macos")]
fn bounded_diagnostic(value: &str, max_chars: usize) -> String {
    let mut output: String = value.chars().take(max_chars).collect();
    if value.chars().count() > max_chars {
        output.push('…');
    }
    output
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_content_on_pre_key_event(
    self_: *mut cef_keyboard_handler_t,
    browser: *mut cef_browser_t,
    event: *const cef_key_event_t_switchboard,
    _os_event: CefEventHandleSwitchboard,
    _is_keyboard_shortcut: *mut c_int,
) -> c_int {
    if self_.is_null() || browser.is_null() || event.is_null() {
        return 0;
    }
    let handler = self_ as *mut SwitchboardContentKeyboardHandler;
    if !remember_browser_for_tab((*handler).tab_id, (*handler).generation, browser) {
        return 0;
    }
    let key_event = &*event;
    if (key_event.type_ != CEF_KEYEVENT_RAWKEYDOWN && key_event.type_ != CEF_KEYEVENT_KEYDOWN)
        || (key_event.modifiers & CEF_EVENTFLAG_IS_REPEAT) != 0
        || fallback_shortcut_action(key_event) != Some(UiShortcutAction::Reload)
    {
        return 0;
    }
    if dispatch_ui_shortcut_action(UiShortcutAction::Reload) {
        return 1;
    }
    0
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_content_on_key_event(
    self_: *mut cef_keyboard_handler_t,
    browser: *mut cef_browser_t,
    event: *const cef_key_event_t_switchboard,
    _os_event: CefEventHandleSwitchboard,
) -> c_int {
    if browser.is_null() || event.is_null() {
        return 0;
    }
    if !self_.is_null() {
        let handler = self_ as *mut SwitchboardContentKeyboardHandler;
        if !remember_browser_for_tab((*handler).tab_id, (*handler).generation, browser) {
            return 0;
        }
    }
    let key_event = &*event;
    if key_event.type_ != CEF_KEYEVENT_RAWKEYDOWN && key_event.type_ != CEF_KEYEVENT_KEYDOWN {
        return 0;
    }
    let browser_mode = WEBSITE_KEYBINDINGS_ACTIVE.with(|slot| slot.get());
    if !browser_mode {
        let escape = event_key_token(key_event) == Some(KeyToken::Escape)
            && key_event.modifiers
                & (CEF_EVENTFLAG_SHIFT_DOWN
                    | CEF_EVENTFLAG_CONTROL_DOWN
                    | CEF_EVENTFLAG_ALT_DOWN
                    | CEF_EVENTFLAG_COMMAND_DOWN)
                == 0;
        return i32::from(
            escape && dispatch_ui_shortcut_action(UiShortcutAction::EnterBrowserMode),
        );
    }
    if key_event.focus_on_editable_field != 0 {
        return 0;
    }
    if (key_event.modifiers & CEF_EVENTFLAG_IS_REPEAT) != 0 {
        return 0;
    }
    if fallback_shortcut_action(key_event) == Some(UiShortcutAction::Reload) {
        return i32::from(dispatch_ui_shortcut_action(UiShortcutAction::Reload));
    }
    let Some(action) = fallback_shortcut_action(key_event) else {
        return 0;
    };
    if dispatch_ui_shortcut_action(action) {
        return 1;
    }
    0
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_content_client_get_display_handler(
    self_: *mut cef_client_t,
) -> *mut cef_display_handler_t {
    if self_.is_null() {
        return std::ptr::null_mut();
    }
    let client = self_ as *mut SwitchboardContentClient;
    (*client).display_handler
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_content_client_get_keyboard_handler(
    self_: *mut cef_client_t,
) -> *mut cef_keyboard_handler_t {
    if self_.is_null() {
        return std::ptr::null_mut();
    }
    let client = self_ as *mut SwitchboardContentClient;
    (*client).keyboard_handler
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn switchboard_content_client_get_load_handler(
    self_: *mut cef_client_t,
) -> *mut cef_load_handler_t {
    if self_.is_null() {
        return std::ptr::null_mut();
    }
    let client = self_ as *mut SwitchboardContentClient;
    (*client).load_handler
}

#[cfg(target_os = "macos")]
fn allocate_content_display_handler(tab_id: TabId, generation: u64) -> *mut cef_display_handler_t {
    let handler = Box::new(SwitchboardContentDisplayHandler {
        handler: cef_display_handler_t {
            base: ref_counted_base::<cef_display_handler_t>(),
            on_address_change: Some(switchboard_content_on_address_change),
            on_title_change: Some(switchboard_content_on_title_change),
            on_favicon_urlchange: None,
            on_fullscreen_mode_change: None,
            on_tooltip: None,
            on_status_message: None,
            on_console_message: None,
            on_auto_resize: None,
            on_loading_progress_change: None,
            on_cursor_change: None,
            on_media_access_change: None,
            on_contents_bounds_change: None,
            get_root_window_screen_rect: None,
        },
        tab_id,
        generation,
    });
    let ptr = Box::into_raw(handler);
    unsafe { &mut (*ptr).handler as *mut cef_display_handler_t }
}

#[cfg(target_os = "macos")]
fn allocate_content_keyboard_handler(
    tab_id: TabId,
    generation: u64,
) -> *mut cef_keyboard_handler_t {
    let handler = Box::new(SwitchboardContentKeyboardHandler {
        handler: cef_keyboard_handler_t_switchboard {
            base: ref_counted_base::<cef_keyboard_handler_t_switchboard>(),
            on_pre_key_event: Some(switchboard_content_on_pre_key_event),
            on_key_event: Some(switchboard_content_on_key_event),
        },
        tab_id,
        generation,
    });
    let ptr = Box::into_raw(handler);
    unsafe {
        &mut (*ptr).handler as *mut cef_keyboard_handler_t_switchboard
            as *mut cef_keyboard_handler_t
    }
}

#[cfg(target_os = "macos")]
fn allocate_content_load_handler(tab_id: TabId, generation: u64) -> *mut cef_load_handler_t {
    let handler = Box::new(SwitchboardContentLoadHandler {
        handler: cef_load_handler_t {
            base: ref_counted_base::<cef_load_handler_t>(),
            on_loading_state_change: Some(switchboard_content_on_loading_state_change),
            on_load_start: Some(switchboard_content_on_load_start),
            on_load_end: Some(switchboard_content_on_load_end),
            on_load_error: Some(switchboard_content_on_load_error),
        },
        tab_id,
        generation,
        main_frame_failed: AtomicBool::new(false),
    });
    let pointer = Box::into_raw(handler);
    unsafe { &mut (*pointer).handler }
}

#[cfg(target_os = "macos")]
fn allocate_content_cef_client(
    profile_id: ProfileId,
    tab_id: TabId,
    generation: u64,
) -> *mut cef_client_t {
    let display_handler = allocate_content_display_handler(tab_id, generation);
    let keyboard_handler = allocate_content_keyboard_handler(tab_id, generation);
    let load_handler = allocate_content_load_handler(tab_id, generation);
    let life_span_handler = allocate_life_span_handler(Some(tab_id), generation);
    let client = Box::new(SwitchboardContentClient {
        client: cef_client_t {
            base: ref_counted_base::<cef_client_t>(),
            get_audio_handler: None,
            get_command_handler: None,
            get_context_menu_handler: None,
            get_dialog_handler: None,
            get_display_handler: Some(switchboard_content_client_get_display_handler),
            get_download_handler: None,
            get_drag_handler: None,
            get_find_handler: None,
            get_focus_handler: None,
            get_frame_handler: None,
            get_permission_handler: None,
            get_jsdialog_handler: None,
            get_keyboard_handler: Some(switchboard_content_client_get_keyboard_handler),
            get_life_span_handler: Some(switchboard_content_client_get_life_span_handler),
            get_load_handler: Some(switchboard_content_client_get_load_handler),
            get_print_handler: None,
            get_render_handler: None,
            get_request_handler: None,
            on_process_message_received: None,
        },
        profile_id,
        tab_id,
        generation,
        display_handler,
        keyboard_handler,
        load_handler,
        life_span_handler,
    });
    let client_ptr = Box::into_raw(client);
    if env_flag(ENV_CEF_VERBOSE_ERRORS) {
        unsafe {
            eprintln!(
                "switchboard-app: allocated content CEF client ptr={client_ptr:p} base.size={}",
                (*client_ptr).client.base.size
            );
        }
    }
    unsafe { &mut (*client_ptr).client as *mut cef_client_t }
}

#[cfg(target_os = "macos")]
unsafe fn free_content_cef_client(client: *mut cef_client_t) {
    if client.is_null() {
        return;
    }
    let content_client = client as *mut SwitchboardContentClient;
    let life_span_handler = (*content_client).life_span_handler;
    if !life_span_handler.is_null() {
        let handler = life_span_handler as *mut SwitchboardLifeSpanHandler;
        if !(*handler).before_close_seen.load(Ordering::Acquire) {
            eprintln!(
                "switchboard-app: refusing to release tab {} generation {} client before OnBeforeClose",
                (*content_client).tab_id.0,
                (*content_client).generation
            );
            return;
        }
    }
    let display_handler = (*content_client).display_handler;
    if !display_handler.is_null() {
        drop(Box::from_raw(
            display_handler as *mut SwitchboardContentDisplayHandler,
        ));
    }
    let keyboard_handler = (*content_client).keyboard_handler;
    if !keyboard_handler.is_null() {
        drop(Box::from_raw(
            keyboard_handler as *mut SwitchboardContentKeyboardHandler,
        ));
    }
    let load_handler = (*content_client).load_handler;
    if !load_handler.is_null() {
        drop(Box::from_raw(
            load_handler as *mut SwitchboardContentLoadHandler,
        ));
    }
    if !life_span_handler.is_null() {
        drop(Box::from_raw(
            life_span_handler as *mut SwitchboardLifeSpanHandler,
        ));
    }
    drop(Box::from_raw(content_client));
}

#[cfg(target_os = "macos")]
unsafe fn free_ui_cef_client(client: *mut cef_client_t) {
    if client.is_null() {
        return;
    }
    let ui_client = client as *mut SwitchboardUiClient;
    let life_span_handler = (*ui_client).life_span_handler;
    if !life_span_handler.is_null() {
        let handler = life_span_handler as *mut SwitchboardLifeSpanHandler;
        if !(*handler).before_close_seen.load(Ordering::Acquire) {
            eprintln!("switchboard-app: refusing to release UI client before OnBeforeClose");
            return;
        }
    }
    let display_handler = (*ui_client).display_handler;
    if !display_handler.is_null() {
        drop(Box::from_raw(
            display_handler as *mut SwitchboardUiDisplayHandler,
        ));
    }
    if !life_span_handler.is_null() {
        drop(Box::from_raw(
            life_span_handler as *mut SwitchboardLifeSpanHandler,
        ));
    }
    drop(Box::from_raw(ui_client));
}

#[cfg(target_os = "macos")]
impl CefRuntime {
    fn from_environment() -> Result<Option<Self>, HostError> {
        let Some(config) = CefConfig::from_env()? else {
            return Ok(None);
        };
        config.validate_for_sandboxed_bundle()?;
        let config_summary = config.summary();
        let verbose_errors = env_flag(ENV_CEF_VERBOSE_ERRORS);

        unsafe {
            let is_subprocess = std::env::args().any(|arg| arg.starts_with("--type="));
            let mut sandbox_argv_storage = if is_subprocess {
                Some(cef_argv_storage(std::env::args())?)
            } else {
                None
            };
            let sandbox_context = if let Some(argv_storage) = sandbox_argv_storage.as_mut() {
                let mut argv_ptrs = cef_argv_ptrs(argv_storage);
                let argc = i32::try_from(argv_ptrs.len())
                    .map_err(|_| HostError::Native("too many argv values for CEF".to_owned()))?;
                let context = CefSandboxContext::initialize(
                    config.sandbox_library_path(),
                    argc,
                    argv_ptrs.as_mut_ptr(),
                )
                .map_err(|error| {
                    HostError::Native(format!(
                        "CEF bootstrap failed at: initialize helper sandbox\nreason: {error}\nconfiguration:\n{config_summary}"
                    ))
                })?;
                if verbose_errors {
                    eprintln!(
                        "switchboard-app: initialized CEF helper sandbox (pid={})",
                        std::process::id()
                    );
                }
                Some(context)
            } else {
                None
            };

            let library = CefLibrary::open(&config.library_path).map_err(|err| {
                let raw_error = err.to_string();
                let reason = short_cef_loader_reason(&raw_error);
                HostError::Native(format!(
                    "CEF bootstrap failed at: load framework\nreason: {reason}\nconfiguration:\n{config_summary}{}{}",
                    if is_gatekeeper_policy_error(&raw_error) {
                        "\nrecommended fix:\n  xattr -dr com.apple.quarantine <cef_dir>"
                    } else {
                        ""
                    },
                    if verbose_errors {
                        format!("\nraw loader detail:\n  {raw_error}")
                    } else {
                        "\nset SWITCHBOARD_CEF_VERBOSE_ERRORS=1 to include raw loader details".to_owned()
                    }
                ))
            })?;
            install_cef_quit_message_loop_hook(library.api.cef_quit_message_loop);
            let app = allocate_cef_app();
            let requested_api_version =
                env_i32(ENV_CEF_API_VERSION).unwrap_or(DEFAULT_CEF_API_VERSION);
            let api_hash = (library.api.cef_api_hash)(requested_api_version, 0);
            if api_hash.is_null() {
                let active_version = (library.api.cef_api_version)();
                return Err(HostError::Native(format!(
                    "CEF bootstrap failed at: configure api version\nreason: cef_api_hash returned null for version {requested_api_version}\n  active_cef_api_version: {active_version}\nconfiguration:\n{config_summary}\nset {ENV_CEF_API_VERSION} to a supported explicit version (for this CEF build, typically {DEFAULT_CEF_API_VERSION})"
                )));
            }
            if verbose_errors {
                let hash_string = CStr::from_ptr(api_hash).to_string_lossy();
                let active_version = (library.api.cef_api_version)();
                eprintln!(
                    "switchboard-app: configured CEF API version={active_version} requested={requested_api_version} platform_hash={hash_string}"
                );
            }

            let mut cef_args: Vec<String> = std::env::args().collect();
            let mock_keychain_requested = env_flag(ENV_CEF_USE_MOCK_KEYCHAIN);
            let smoke_bundle = std::env::current_exe()
                .ok()
                .is_some_and(|path| path.to_string_lossy().contains("Switchboard Smoke.app/"));
            if mock_keychain_requested && !smoke_bundle {
                return Err(HostError::Native(format!(
                    "{ENV_CEF_USE_MOCK_KEYCHAIN} is restricted to the dedicated Switchboard Smoke.app bundle"
                )));
            }
            let use_mock_keychain = mock_keychain_requested && smoke_bundle;
            if use_mock_keychain {
                upsert_cef_switch(&mut cef_args, "--use-mock-keychain");
            }
            if let Ok(password_store) = std::env::var(ENV_CEF_PASSWORD_STORE) {
                let trimmed = password_store.trim();
                if !trimmed.is_empty() {
                    upsert_cef_switch_with_value(&mut cef_args, "--password-store", trimmed);
                }
            }
            let autoplay_policy = std::env::var(ENV_CEF_AUTOPLAY_POLICY)
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| DEFAULT_CEF_AUTOPLAY_POLICY.to_owned());
            upsert_cef_switch_with_value(&mut cef_args, "--autoplay-policy", &autoplay_policy);
            if verbose_errors {
                eprintln!(
                    "switchboard-app: CEF launch switches mock_keychain={} password_store={} autoplay_policy={}",
                    use_mock_keychain,
                    std::env::var(ENV_CEF_PASSWORD_STORE).unwrap_or_else(|_| "unset".to_owned()),
                    autoplay_policy
                );
            }

            let argv_storage = cef_argv_storage(cef_args.into_iter())?;
            let mut argv_ptrs = cef_argv_ptrs(&argv_storage);
            let main_args = cef_main_args_t {
                argc: i32::try_from(argv_ptrs.len())
                    .map_err(|_| HostError::Native("too many argv values for CEF".to_owned()))?,
                argv: argv_ptrs.as_mut_ptr(),
            };

            let secondary_exit_code =
                (library.api.cef_execute_process)(&main_args, app, std::ptr::null_mut());
            if verbose_errors {
                eprintln!(
                    "switchboard-app: cef_execute_process returned {secondary_exit_code} (pid={})",
                    std::process::id()
                );
            }
            if secondary_exit_code >= 0 {
                // `process::exit` does not run destructors. CEF requires the
                // framework to unload before the helper sandbox is destroyed.
                drop(library);
                drop(sandbox_context);
                std::process::exit(secondary_exit_code);
            }
            eprintln!("switchboard-app: CEF bootstrap configuration\n{config_summary}");

            let mut settings: cef_settings_t = zeroed();
            settings.size = size_of::<cef_settings_t>();
            settings.no_sandbox = 0;
            settings.background_color = DEFAULT_BACKGROUND_COLOR;

            std::fs::create_dir_all(&config.root_cache_path).map_err(|error| {
                HostError::Native(format!(
                    "failed creating CEF root_cache_path {}: {error}",
                    config.root_cache_path.display()
                ))
            })?;
            std::fs::create_dir_all(&config.cache_path).map_err(|error| {
                HostError::Native(format!(
                    "failed creating CEF cache_path {}: {error}",
                    config.cache_path.display()
                ))
            })?;
            std::fs::create_dir_all(&config.temp_dir).map_err(|error| {
                HostError::Native(format!(
                    "failed creating CEF temp_dir {}: {error}",
                    config.temp_dir.display()
                ))
            })?;
            std::env::set_var("TMPDIR", &config.temp_dir);
            std::env::set_var("TEMP", &config.temp_dir);
            std::env::set_var("TMP", &config.temp_dir);
            let mut settings_strings = Vec::new();
            set_cef_path_string(
                &library,
                &mut settings.framework_dir_path,
                &config.framework_dir_path,
                &mut settings_strings,
            )?;
            set_cef_path_string(
                &library,
                &mut settings.root_cache_path,
                &config.root_cache_path,
                &mut settings_strings,
            )?;
            set_cef_path_string(
                &library,
                &mut settings.cache_path,
                &config.cache_path,
                &mut settings_strings,
            )?;
            if let Some(path) = config.resources_dir_path.as_ref() {
                set_cef_path_string(
                    &library,
                    &mut settings.resources_dir_path,
                    path,
                    &mut settings_strings,
                )?;
            }
            if let Some(path) = config.browser_subprocess_path.as_ref() {
                set_cef_path_string(
                    &library,
                    &mut settings.browser_subprocess_path,
                    path,
                    &mut settings_strings,
                )?;
            }
            if let Some(path) = config.main_bundle_path.as_ref() {
                set_cef_path_string(
                    &library,
                    &mut settings.main_bundle_path,
                    path,
                    &mut settings_strings,
                )?;
            }

            let initialized =
                (library.api.cef_initialize)(&main_args, &settings, app, std::ptr::null_mut());
            drop(settings_strings);
            if initialized == 0 {
                return Err(HostError::Native(format!(
                    "CEF bootstrap failed at: initialize\nreason: cef_initialize returned false\nconfiguration:\n{config_summary}\nnotes:\n  - if logs mention ProcessSingleton/SingletonSocket, ensure tmp/cache paths are writable\n  - override with SWITCHBOARD_CEF_ROOT_CACHE_PATH and SWITCHBOARD_CEF_TMPDIR if needed"
                )));
            }
            if !UI_SCHEME_DECLARED.load(Ordering::Acquire) {
                return Err(HostError::Native(
                    "CEF bootstrap failed at: declare app:// scheme\nreason: OnRegisterCustomSchemes was not accepted in the browser process"
                        .to_owned(),
                ));
            }

            let app_scheme = CefString::new(&library, UI_SCHEME)?;
            let ui_domain = CefString::new(&library, "ui")?;
            let ui_scheme_factory = allocate_ui_scheme_factory();
            let ui_request_context = create_cef_request_context(&library, None, false, false)?;
            let register_scheme_handler_factory = (*ui_request_context)
                .register_scheme_handler_factory
                .ok_or_else(|| {
                    HostError::Native(
                        "CEF UI request context does not expose scheme registration".to_owned(),
                    )
                })?;
            let registered = register_scheme_handler_factory(
                ui_request_context,
                app_scheme.as_ptr(),
                ui_domain.as_ptr(),
                ui_scheme_factory,
            );
            if registered == 0 {
                return Err(HostError::Native(
                    "CEF bootstrap failed at: register app:// scheme handler on the isolated UI request context"
                        .to_owned(),
                ));
            }
            if verbose_errors {
                eprintln!("switchboard-app: registered app:// handler on isolated UI context");
            }
            let ui_client = allocate_ui_cef_client();

            Ok(Some(Self {
                library,
                config,
                _app: app,
                _ui_scheme_factory: ui_scheme_factory,
                ui_client,
                ui_request_context,
                profile_request_contexts: RefCell::new(HashMap::new()),
            }))
        }
    }

    fn create_browser_in_view(
        &self,
        parent_view: ObjcId,
        url: &str,
        client: *mut cef_client_t,
        request_context: *mut cef_request_context_t,
        width: f64,
        height: f64,
    ) -> Result<(), HostError> {
        unsafe {
            if env_flag(ENV_CEF_VERBOSE_ERRORS) {
                eprintln!(
                    "switchboard-app: creating CEF browser parent_view={parent_view:p} client={client:p}"
                );
            }
            let mut window_info: cef_window_info_t = zeroed();
            window_info.size = size_of::<cef_window_info_t>();
            window_info.bounds = cef_rect_t {
                x: 0,
                y: 0,
                width: width as c_int,
                height: height as c_int,
            };
            window_info.parent_view = parent_view;
            window_info.runtime_style = CEF_RUNTIME_STYLE_ALLOY;

            let mut browser_settings: cef_browser_settings_t = zeroed();
            browser_settings.size = size_of::<cef_browser_settings_t>();
            browser_settings.background_color = DEFAULT_BACKGROUND_COLOR;

            let url_value = CefString::new(&self.library, url)?;
            let create_browser: cef_browser_host_create_browser_fn =
                self.library.api.cef_browser_host_create_browser;
            // Ref-counted C API parameters use transfer semantics. Transfer
            // additional references so the host-owned client and persistent
            // profile request context remain valid after this call returns.
            retain_cef_client(client);
            retain_cef_request_context(request_context);
            let result = create_browser(
                &window_info,
                client,
                url_value.as_ptr(),
                &browser_settings,
                std::ptr::null_mut(),
                request_context,
            );
            if env_flag(ENV_CEF_VERBOSE_ERRORS) {
                eprintln!(
                    "switchboard-app: cef_browser_host_create_browser returned {result} for url={url}"
                );
            }

            if result == 0 {
                let config_summary = self.config.summary();
                return Err(HostError::Native(format!(
                    "CEF browser creation failed\nreason: cef_browser_host_create_browser returned 0\n  url               : {url}\n  parent_view       : {parent_view:p}\nconfiguration:\n{config_summary}"
                )));
            }
            Ok(())
        }
    }

    fn ui_client(&self) -> *mut cef_client_t {
        self.ui_client
    }

    fn ui_request_context(&self) -> *mut cef_request_context_t {
        self.ui_request_context
    }

    fn request_context_for_profile(
        &self,
        profile_id: ProfileId,
    ) -> Result<*mut cef_request_context_t, HostError> {
        if let Some(context) = self
            .profile_request_contexts
            .borrow()
            .get(&profile_id)
            .copied()
        {
            if env_flag(ENV_CEF_VERBOSE_ERRORS) {
                eprintln!(
                    "switchboard-app: reusing profile {} request context {context:p}",
                    profile_id.0
                );
            }
            return Ok(context);
        }
        let path = profile_cache_path(&self.config.root_cache_path, profile_id);
        std::fs::create_dir_all(&path).map_err(|error| {
            HostError::Native(format!(
                "failed creating profile {} CEF cache {}: {error}",
                profile_id.0,
                path.display()
            ))
        })?;
        let context = create_cef_request_context(&self.library, Some(&path), true, true)?;
        if env_flag(ENV_CEF_VERBOSE_ERRORS) {
            eprintln!(
                "switchboard-app: created profile {} request context {context:p}",
                profile_id.0
            );
        }
        self.profile_request_contexts
            .borrow_mut()
            .insert(profile_id, context);
        Ok(context)
    }

    fn run_message_loop(&self) {
        unsafe {
            (self.library.api.cef_run_message_loop)();
        }
    }

    fn shutdown(&self) {
        unsafe {
            for context in self
                .profile_request_contexts
                .borrow_mut()
                .drain()
                .map(|(_, value)| value)
            {
                release_cef_request_context(context);
            }
            release_cef_request_context(self.ui_request_context);
        }
        unsafe {
            (self.library.api.cef_shutdown)();
        }
        clear_cef_quit_message_loop_hook();
    }
}

#[cfg(target_os = "macos")]
fn profile_cache_path(root_cache_path: &Path, profile_id: ProfileId) -> PathBuf {
    root_cache_path.join(format!("profile-{}", profile_id.0))
}

#[cfg(target_os = "macos")]
fn create_cef_request_context(
    library: &CefLibrary,
    cache_path: Option<&Path>,
    persist_session_cookies: bool,
    allow_default_cookie_schemes: bool,
) -> Result<*mut cef_request_context_t, HostError> {
    unsafe {
        let mut settings: cef_request_context_settings_t = zeroed();
        settings.size = size_of::<cef_request_context_settings_t>();
        settings.persist_session_cookies = i32::from(persist_session_cookies);
        settings.cookieable_schemes_exclude_defaults = i32::from(!allow_default_cookie_schemes);
        let cache = cache_path
            .map(|path| CefString::new(library, &path.to_string_lossy()))
            .transpose()?;
        if let Some(cache) = cache.as_ref() {
            settings.cache_path = cache.value();
        }
        let context =
            (library.api.cef_request_context_create_context)(&settings, std::ptr::null_mut());
        if context.is_null() {
            return Err(HostError::Native(format!(
                "CEF failed to create {} request context",
                if cache_path.is_some() {
                    "profile"
                } else {
                    "ephemeral UI"
                }
            )));
        }
        Ok(context)
    }
}

#[cfg(target_os = "macos")]
unsafe fn release_cef_request_context(context: *mut cef_request_context_t) {
    if context.is_null() {
        return;
    }
    if let Some(release) = (*context).base.base.release {
        release(&mut (*context).base.base);
    }
}

#[cfg(target_os = "macos")]
unsafe fn retain_cef_request_context(context: *mut cef_request_context_t) {
    if context.is_null() {
        return;
    }
    if let Some(add_ref) = (*context).base.base.add_ref {
        add_ref(&mut (*context).base.base);
    }
}

#[cfg(target_os = "macos")]
unsafe fn retain_cef_client(client: *mut cef_client_t) {
    if client.is_null() {
        return;
    }
    if let Some(add_ref) = (*client).base.add_ref {
        add_ref(&mut (*client).base);
    }
}

#[cfg(target_os = "macos")]
fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

#[cfg(target_os = "macos")]
fn default_subprocess_path() -> Result<PathBuf, HostError> {
    std::env::current_exe().map_err(|error| {
        HostError::Native(format!(
            "failed to resolve current executable for browser_subprocess_path: {error}"
        ))
    })
}

#[cfg(target_os = "macos")]
fn default_root_cache_path() -> Result<PathBuf, HostError> {
    let user_home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
        HostError::Native("HOME is not set; cannot resolve CEF cache path".to_owned())
    })?;
    Ok(user_home
        .join("Library")
        .join("Application Support")
        .join("Switchboard")
        .join("CEF"))
}

#[cfg(target_os = "macos")]
fn default_temp_dir() -> PathBuf {
    std::env::temp_dir().join("switchboard_cef_tmp")
}

#[cfg(target_os = "macos")]
fn describe_path(path: &Path) -> String {
    format!(
        "{} [exists: {}]",
        path.display(),
        if path.exists() { "yes" } else { "no" }
    )
}

#[cfg(target_os = "macos")]
fn describe_optional_path(path: &Option<PathBuf>) -> String {
    match path {
        Some(path) => describe_path(path),
        None => "unset".to_owned(),
    }
}

#[cfg(target_os = "macos")]
fn env_flag(key: &str) -> bool {
    std::env::var(key)
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn env_i32(key: &str) -> Option<i32> {
    std::env::var(key).ok().and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            trimmed.parse::<i32>().ok()
        }
    })
}

#[cfg(target_os = "macos")]
fn short_cef_loader_reason(raw_error: &str) -> String {
    if is_gatekeeper_policy_error(raw_error) {
        return "macOS Gatekeeper/code-signing policy blocked the CEF framework".to_owned();
    }

    raw_error
        .split(": tried:")
        .next()
        .unwrap_or(raw_error)
        .trim()
        .to_owned()
}

#[cfg(target_os = "macos")]
fn is_gatekeeper_policy_error(raw_error: &str) -> bool {
    raw_error.contains("library load disallowed by system policy")
        || raw_error.contains("not valid for use in process")
}

#[cfg(target_os = "macos")]
fn detect_cef_helper_binary(release_dir: &Path) -> Option<PathBuf> {
    let candidates = [
        "cefclient.app/Contents/Frameworks/cefclient Helper.app/Contents/MacOS/cefclient Helper",
        "cefsimple.app/Contents/Frameworks/cefsimple Helper.app/Contents/MacOS/cefsimple Helper",
        "ceftests.app/Contents/Frameworks/ceftests Helper.app/Contents/MacOS/ceftests Helper",
    ];
    for candidate in candidates {
        let path = release_dir.join(candidate);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn set_cef_path_string(
    library: &CefLibrary,
    target: &mut cef_string_t,
    path: &Path,
    keep_alive: &mut Vec<CefString>,
) -> Result<(), HostError> {
    let value = CefString::new(library, &path.to_string_lossy())?;
    *target = value.value();
    keep_alive.push(value);
    Ok(())
}

#[cfg(target_os = "macos")]
fn upsert_cef_switch(args: &mut Vec<String>, switch: &str) {
    if args.iter().any(|arg| arg == switch) {
        return;
    }
    args.push(switch.to_owned());
}

#[cfg(target_os = "macos")]
fn upsert_cef_switch_with_value(args: &mut Vec<String>, switch: &str, value: &str) {
    let prefix = format!("{switch}=");
    if let Some(existing) = args.iter_mut().find(|arg| arg.starts_with(&prefix)) {
        *existing = format!("{prefix}{value}");
        return;
    }
    args.push(format!("{prefix}{value}"));
}

#[cfg(target_os = "macos")]
fn cef_argv_storage(args: impl IntoIterator<Item = String>) -> Result<Vec<CString>, HostError> {
    args.into_iter()
        .map(|arg| {
            CString::new(arg)
                .map_err(|_| HostError::Native("argv contained interior NUL".to_owned()))
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn cef_argv_ptrs(argv_storage: &[CString]) -> Vec<*mut c_char> {
    argv_storage
        .iter()
        .map(|arg| arg.as_ptr() as *mut c_char)
        .collect()
}

#[cfg(target_os = "macos")]
pub struct NativeMacHost {
    app: ObjcId,
    cef: Option<CefRuntime>,
    next_window_id: u64,
    next_ui_view_id: u64,
    next_content_view_id: u64,
    windows: HashMap<WindowId, ObjcId>,
    ui_views: HashMap<UiViewId, ObjcId>,
    content_views: HashMap<ContentViewId, ContentBackend>,
    content_view_tabs: HashMap<ContentViewId, TabId>,
    content_view_windows: HashMap<ContentViewId, WindowId>,
    cef_clients: HashMap<ContentViewId, *mut cef_client_t>,
    retired_cef_clients: Vec<*mut cef_client_t>,
    focus_mode: bool,
}

#[cfg(target_os = "macos")]
impl NativeMacHost {
    pub fn new() -> Result<Self, HostError> {
        let is_cef_subprocess = std::env::args().any(|arg| arg.starts_with("--type="));
        // Initialize CEF first so subprocesses can return from cef_execute_process
        // and exit before any Cocoa app activation/Dock integration happens.
        let cef = CefRuntime::from_environment()?.ok_or_else(|| {
            HostError::Native(
                "Switchboard must be launched from its generated macOS .app bundle; bare cargo run is unsupported"
                    .to_owned(),
            )
        })?;
        if is_cef_subprocess {
            return Err(HostError::Native(
                "CEF subprocess reached UI host initialization unexpectedly".to_owned(),
            ));
        }

        unsafe {
            if NSApplicationLoad() == NO {
                return Err(HostError::Native("failed to load NSApplication".to_owned()));
            }
            install_nsapplication_event_shim()?;
            install_nsapplication_suspend_shim()?;

            let app_class = objc_class("NSApplication")?;
            let app = msg_send_id(app_class, selector("sharedApplication")?);
            if app == NIL {
                return Err(HostError::Native(
                    "NSApplication sharedApplication returned nil".to_owned(),
                ));
            }
            msg_send_void_id(app, selector("setDelegate:")?, app);

            msg_send_void_i64(
                app,
                selector("setActivationPolicy:")?,
                APP_ACTIVATION_POLICY_REGULAR,
            );
            msg_send_void(app, selector("finishLaunching")?);

            Ok(Self {
                app,
                cef: Some(cef),
                next_window_id: 0,
                next_ui_view_id: 0,
                next_content_view_id: 0,
                windows: HashMap::new(),
                ui_views: HashMap::new(),
                content_views: HashMap::new(),
                content_view_tabs: HashMap::new(),
                content_view_windows: HashMap::new(),
                cef_clients: HashMap::new(),
                retired_cef_clients: Vec::new(),
                focus_mode: false,
            })
        }
    }

    fn window_for(&self, window_id: WindowId) -> Result<ObjcId, HostError> {
        self.windows
            .get(&window_id)
            .copied()
            .ok_or_else(|| HostError::Native(format!("window not found: {}", window_id.0)))
    }

    fn browser_for_content_view(
        &self,
        view_id: ContentViewId,
    ) -> Result<*mut cef_browser_t, HostError> {
        let tab_id = self
            .content_view_tabs
            .get(&view_id)
            .copied()
            .ok_or_else(|| HostError::Native(format!("content view not found: {}", view_id.0)))?;
        let browser = browser_for_tab(tab_id);
        if browser.is_null() {
            return Err(HostError::Native(format!(
                "CEF browser is not ready for tab {}",
                tab_id.0
            )));
        }
        Ok(browser)
    }
}

#[cfg(test)]
mod tests {
    use super::is_exact_ui_url;
    #[cfg(target_os = "macos")]
    use super::{
        allocate_content_load_handler, browser_callback_action, main_app_bundle_for_executable,
        profile_cache_path, BrowserCallbackAction, SwitchboardContentLoadHandler,
    };
    #[cfg(target_os = "macos")]
    use std::path::Path;
    #[cfg(target_os = "macos")]
    use switchboard_core::{ProfileId, TabId};

    #[test]
    fn privileged_ui_origin_match_is_exact() {
        assert!(is_exact_ui_url("app://ui"));
        assert!(is_exact_ui_url("app://ui/"));
        assert!(is_exact_ui_url("app://ui?v=1#bridge=x"));
        assert!(!is_exact_ui_url("app://ui.evil.example"));
        assert!(!is_exact_ui_url("https://ui"));
        assert!(!is_exact_ui_url("app://content"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn nested_helpers_resolve_the_outer_application_bundle() {
        let main = Path::new("/tmp/Switchboard Dev.app/Contents/MacOS/switchboard-app");
        assert_eq!(
            main_app_bundle_for_executable(main).as_deref(),
            Some(Path::new("/tmp/Switchboard Dev.app"))
        );

        let helper = Path::new(
            "/tmp/Switchboard Dev.app/Contents/Frameworks/Switchboard Helper (GPU).app/Contents/MacOS/Switchboard Helper (GPU)",
        );
        assert_eq!(
            main_app_bundle_for_executable(helper).as_deref(),
            Some(Path::new("/tmp/Switchboard Dev.app"))
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn browser_callback_attribution_rejects_stale_generations_and_refreshes_wrappers() {
        assert_eq!(
            browser_callback_action(None, 4, false, false),
            BrowserCallbackAction::Insert
        );
        assert_eq!(
            browser_callback_action(Some(4), 3, false, true),
            BrowserCallbackAction::Reject
        );
        assert_eq!(
            browser_callback_action(Some(4), 4, true, true),
            BrowserCallbackAction::Keep
        );
        assert_eq!(
            browser_callback_action(Some(4), 4, false, true),
            BrowserCallbackAction::RefreshWrapper
        );
        assert_eq!(
            browser_callback_action(Some(4), 4, false, false),
            BrowserCallbackAction::Reject
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn content_load_handler_installs_all_cef_145_callbacks() {
        let pointer = allocate_content_load_handler(TabId(7), 11);
        assert!(!pointer.is_null());
        unsafe {
            assert!((*pointer).on_loading_state_change.is_some());
            assert!((*pointer).on_load_start.is_some());
            assert!((*pointer).on_load_end.is_some());
            assert!((*pointer).on_load_error.is_some());
            drop(Box::from_raw(pointer as *mut SwitchboardContentLoadHandler));
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn profile_cache_is_a_direct_child_of_the_cef_root() {
        let root = Path::new("/private/tmp/switchboard-cef-root");
        let path = profile_cache_path(root, ProfileId(9));
        assert_eq!(path, root.join("profile-9"));
        assert_eq!(path.parent(), Some(root));
    }
}

#[cfg(target_os = "macos")]
impl CefHost for NativeMacHost {
    type Error = HostError;

    fn create_window(&mut self, title: &str, size: WindowSize) -> Result<WindowId, Self::Error> {
        unsafe {
            let style = STYLE_TITLED
                | STYLE_CLOSABLE
                | STYLE_MINIATURIZABLE
                | STYLE_RESIZABLE
                | STYLE_FULL_SIZE_CONTENT_VIEW;
            let width = f64::from(size.width.max(1));
            let height = f64::from(size.height.max(1));
            let frame = NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size: NSSize { width, height },
            };

            let window_class = objc_class("NSWindow")?;
            let window_alloc = msg_send_id(window_class, selector("alloc")?);
            let window = msg_send_id_rect_u64_u64_bool(
                window_alloc,
                selector("initWithContentRect:styleMask:backing:defer:")?,
                frame,
                style,
                BACKING_STORE_BUFFERED,
                NO,
            );
            if window == NIL {
                return Err(HostError::Native("failed to create NSWindow".to_owned()));
            }

            msg_send_void_bool(window, selector("setTitlebarAppearsTransparent:")?, YES);
            msg_send_void_i64(
                window,
                selector("setTitleVisibility:")?,
                WINDOW_TITLE_HIDDEN,
            );
            msg_send_void_bool(window, selector("setMovableByWindowBackground:")?, YES);
            msg_send_void_id(window, selector("setDelegate:")?, self.app);
            msg_send_void(window, selector("center")?);
            let title_value = nsstring(title)?;
            msg_send_void_id(window, selector("setTitle:")?, title_value);
            msg_send_void_id(window, selector("makeKeyAndOrderFront:")?, NIL);
            msg_send_void_bool(self.app, selector("activateIgnoringOtherApps:")?, YES);

            self.next_window_id += 1;
            let window_id = WindowId(self.next_window_id);
            self.windows.insert(window_id, window);
            Ok(window_id)
        }
    }

    fn create_ui_view(&mut self, window_id: WindowId, url: &str) -> Result<UiViewId, Self::Error> {
        if !is_exact_ui_url(url) {
            return Err(HostError::InvalidUiUrl(url.to_owned()));
        }
        let cef = self.cef.as_ref().ok_or_else(|| {
            HostError::Native(
                "UI shell requires CEF. Set SWITCHBOARD_CEF_DIST or SWITCHBOARD_CEF_LIBRARY."
                    .to_owned(),
            )
        })?;

        unsafe {
            let window = self.window_for(window_id)?;
            let root_view = msg_send_id(window, selector("contentView")?);
            if root_view == NIL {
                return Err(HostError::Native("window content view is nil".to_owned()));
            }
            msg_send_void_bool(root_view, selector("setAutoresizesSubviews:")?, YES);
            let (root_width, root_height) = current_view_size(root_view)?;

            let view_class = objc_class("NSView")?;
            let view_alloc = msg_send_id(view_class, selector("alloc")?);
            let frame = NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size: NSSize {
                    width: root_width,
                    height: root_height,
                },
            };
            let ui_view = msg_send_id_rect(view_alloc, selector("initWithFrame:")?, frame);
            if ui_view == NIL {
                return Err(HostError::Native("failed to create UI view".to_owned()));
            }
            msg_send_void_u64(
                ui_view,
                selector("setAutoresizingMask:")?,
                UI_VIEW_AUTORE_SIZE_MASK,
            );

            msg_send_void_id(root_view, selector("addSubview:")?, ui_view);
            set_ui_view_handles(root_view, ui_view);
            cef.create_browser_in_view(
                ui_view,
                url,
                cef.ui_client(),
                cef.ui_request_context(),
                root_width,
                root_height,
            )?;

            self.next_ui_view_id += 1;
            let view_id = UiViewId(self.next_ui_view_id);
            self.ui_views.insert(view_id, ui_view);
            Ok(view_id)
        }
    }

    fn create_content_view(
        &mut self,
        window_id: WindowId,
        profile_id: ProfileId,
        tab_id: TabId,
        generation: u64,
        url: &str,
    ) -> Result<ContentViewId, Self::Error> {
        if url.starts_with("app://") {
            return Err(HostError::InvalidContentUrl(url.to_owned()));
        }

        unsafe {
            let window = self.window_for(window_id)?;
            let root_view = msg_send_id(window, selector("contentView")?);
            if root_view == NIL {
                return Err(HostError::Native("window content view is nil".to_owned()));
            }
            msg_send_void_bool(root_view, selector("setAutoresizesSubviews:")?, YES);
            let frame = content_container_frame(root_view)?;
            let content_view = create_content_container(frame)?;
            msg_send_void_u64(
                content_view,
                selector("setAutoresizingMask:")?,
                CONTENT_VIEW_AUTORE_SIZE_MASK,
            );
            msg_send_void_id(root_view, selector("addSubview:")?, content_view);

            let mut cef_client: Option<*mut cef_client_t> = None;
            let backend = if let Some(cef) = self.cef.as_ref() {
                set_active_content_browser(std::ptr::null_mut());
                let client = allocate_content_cef_client(profile_id, tab_id, generation);
                let request_context = cef.request_context_for_profile(profile_id)?;
                if let Err(error) = cef.create_browser_in_view(
                    content_view,
                    url,
                    client,
                    request_context,
                    frame.size.width,
                    frame.size.height,
                ) {
                    free_content_cef_client(client);
                    return Err(error);
                }
                set_active_content_tab(Some(tab_id));
                cef_client = Some(client);
                ContentBackend::Cef(content_view)
            } else {
                attach_wk_web_view(content_view, url)?;
                set_active_content_tab(Some(tab_id));
                ContentBackend::WebKit(content_view)
            };

            let title_value = nsstring(&format!("Switchboard - {url}"))?;
            msg_send_void_id(window, selector("setTitle:")?, title_value);

            self.next_content_view_id += 1;
            let view_id = ContentViewId(self.next_content_view_id);
            self.content_views.insert(view_id, backend);
            self.content_view_tabs.insert(view_id, tab_id);
            self.content_view_windows.insert(view_id, window_id);
            if let Some(client) = cef_client {
                self.cef_clients.insert(view_id, client);
            }
            Ok(view_id)
        }
    }

    fn navigate_content_view(
        &mut self,
        view_id: ContentViewId,
        tab_id: TabId,
        url: &str,
    ) -> Result<(), Self::Error> {
        if url.starts_with("app://") {
            return Err(HostError::InvalidContentUrl(url.to_owned()));
        }

        let window_id = self
            .content_view_windows
            .get(&view_id)
            .copied()
            .ok_or_else(|| HostError::Native(format!("content view not found: {}", view_id.0)))?;

        unsafe {
            let window = self.window_for(window_id)?;
            let content_backend = self.content_views.get(&view_id).copied().ok_or_else(|| {
                HostError::Native(format!("content view not found: {}", view_id.0))
            })?;
            match content_backend {
                ContentBackend::WebKit(view) => {
                    attach_wk_web_view(view, url)?;
                    set_active_content_tab(Some(tab_id));
                }
                ContentBackend::Cef(view) => {
                    let _ = view;
                    let browser = self.browser_for_content_view(view_id)?;
                    let get_frame = (*browser).get_main_frame.ok_or_else(|| {
                        HostError::Native("CEF get_main_frame callback unavailable".to_owned())
                    })?;
                    let frame = get_frame(browser);
                    if frame.is_null() {
                        return Err(HostError::Native("CEF main frame unavailable".to_owned()));
                    }
                    let load_url = (*frame).load_url.ok_or_else(|| {
                        HostError::Native("CEF frame load_url callback unavailable".to_owned())
                    })?;
                    with_stack_cef_string(url, |value| load_url(frame, value));
                    set_active_content_tab(Some(tab_id));
                }
            }
            let title_value = nsstring(&format!("Switchboard - {url}"))?;
            msg_send_void_id(window, selector("setTitle:")?, title_value);
        }
        self.content_view_tabs.insert(view_id, tab_id);
        Ok(())
    }

    fn set_content_view_visible(
        &mut self,
        view_id: ContentViewId,
        visible: bool,
    ) -> Result<(), Self::Error> {
        let content_backend =
            self.content_views.get(&view_id).copied().ok_or_else(|| {
                HostError::Native(format!("content view not found: {}", view_id.0))
            })?;
        let container = match content_backend {
            ContentBackend::WebKit(view) | ContentBackend::Cef(view) => view,
        };

        unsafe {
            msg_send_void_bool(
                container,
                selector("setHidden:")?,
                if visible { NO } else { YES },
            );
        }
        if visible {
            set_active_content_container(container)?;
            if let Some(tab_id) = self.content_view_tabs.get(&view_id).copied() {
                set_active_content_tab(Some(tab_id));
                let mapped_browser = browser_for_tab(tab_id);
                if !mapped_browser.is_null() {
                    set_active_content_browser(mapped_browser);
                } else {
                    set_active_content_browser(std::ptr::null_mut());
                }
            }
        } else {
            clear_active_content_container_if_matches(container)?;
        }
        Ok(())
    }

    fn layout_content_views(
        &mut self,
        primary: Option<ContentViewId>,
        secondary: Option<ContentViewId>,
        ratio: f64,
        split_enabled: bool,
    ) -> Result<(), Self::Error> {
        let Some(primary_id) = primary else {
            return Ok(());
        };
        let primary_backend = self
            .content_views
            .get(&primary_id)
            .copied()
            .ok_or_else(|| {
                HostError::Native(format!("content view not found: {}", primary_id.0))
            })?;
        let primary_container = match primary_backend {
            ContentBackend::WebKit(view) | ContentBackend::Cef(view) => view,
        };
        let window_id = self
            .content_view_windows
            .get(&primary_id)
            .copied()
            .ok_or_else(|| HostError::Native("primary content window is missing".to_owned()))?;
        unsafe {
            let window = self.window_for(window_id)?;
            let root = msg_send_id(window, selector("contentView")?);
            let full = if self.focus_mode {
                msg_send_rect(root, selector("bounds")?)
            } else {
                content_container_frame(root)?
            };
            if split_enabled {
                let clamped = ratio.clamp(0.25, 0.75);
                let primary_width = (full.size.width * clamped).max(1.0);
                msg_send_void_rect(
                    primary_container,
                    selector("setFrame:")?,
                    NSRect {
                        origin: full.origin,
                        size: NSSize {
                            width: primary_width,
                            height: full.size.height,
                        },
                    },
                );
            }
            if let Some(secondary_id) = secondary.filter(|_| split_enabled) {
                let secondary_backend =
                    self.content_views
                        .get(&secondary_id)
                        .copied()
                        .ok_or_else(|| {
                            HostError::Native(format!("content view not found: {}", secondary_id.0))
                        })?;
                let secondary_container = match secondary_backend {
                    ContentBackend::WebKit(view) | ContentBackend::Cef(view) => view,
                };
                let primary_width = (full.size.width * ratio.clamp(0.25, 0.75)).max(1.0);
                let secondary_width = (full.size.width - primary_width).max(1.0);
                msg_send_void_rect(
                    secondary_container,
                    selector("setFrame:")?,
                    NSRect {
                        origin: NSPoint {
                            x: full.origin.x + primary_width,
                            y: full.origin.y,
                        },
                        size: NSSize {
                            width: secondary_width,
                            height: full.size.height,
                        },
                    },
                );
            } else if !split_enabled {
                msg_send_void_rect(primary_container, selector("setFrame:")?, full);
            }
        }
        Ok(())
    }

    fn set_focus_mode(&mut self, active: bool) -> Result<(), Self::Error> {
        self.focus_mode = active;
        set_ui_overlay_visible(!active)
    }

    fn go_back(&mut self, view_id: ContentViewId) -> Result<(), Self::Error> {
        let browser = self.browser_for_content_view(view_id)?;
        unsafe {
            let callback = (*browser)
                .go_back
                .ok_or_else(|| HostError::Native("CEF go_back callback unavailable".to_owned()))?;
            callback(browser);
        }
        Ok(())
    }

    fn go_forward(&mut self, view_id: ContentViewId) -> Result<(), Self::Error> {
        let browser = self.browser_for_content_view(view_id)?;
        unsafe {
            let callback = (*browser).go_forward.ok_or_else(|| {
                HostError::Native("CEF go_forward callback unavailable".to_owned())
            })?;
            callback(browser);
        }
        Ok(())
    }

    fn reload(&mut self, view_id: ContentViewId) -> Result<(), Self::Error> {
        let browser = self.browser_for_content_view(view_id)?;
        unsafe {
            let callback = (*browser)
                .reload
                .ok_or_else(|| HostError::Native("CEF reload callback unavailable".to_owned()))?;
            callback(browser);
        }
        Ok(())
    }

    fn send_ui_message(&mut self, payload: &str) -> Result<(), Self::Error> {
        let browser = ui_shell_browser();
        if browser.is_null() {
            return Ok(());
        }
        unsafe {
            let get_frame = (*browser)
                .get_main_frame
                .ok_or_else(|| HostError::Native("CEF UI get_main_frame unavailable".to_owned()))?;
            let frame = get_frame(browser);
            if frame.is_null() {
                return Err(HostError::Native(
                    "CEF UI main frame unavailable".to_owned(),
                ));
            }
            let execute = (*frame).execute_java_script.ok_or_else(|| {
                HostError::Native("CEF execute_java_script unavailable".to_owned())
            })?;
            let literal = serde_json::to_string(payload).map_err(|error| {
                HostError::Native(format!("failed to encode UI message: {error}"))
            })?;
            let script =
                format!("window.switchboardReceive && window.switchboardReceive({literal});");
            let script_url = "app://ui/bridge";
            with_stack_cef_string(&script, |code| {
                with_stack_cef_string(script_url, |url| execute(frame, code, url, 1));
            });
        }
        Ok(())
    }

    fn toggle_dev_tools_for_active_content(&mut self) -> Result<(), Self::Error> {
        if self.cef.is_none() {
            return Err(HostError::Native(
                "DevTools require CEF runtime. Set SWITCHBOARD_CEF_DIST or SWITCHBOARD_CEF_LIBRARY."
                    .to_owned(),
            ));
        }

        let browser = active_content_tab()
            .map(browser_for_tab)
            .filter(|ptr| !ptr.is_null())
            .unwrap_or_else(active_content_browser);
        if browser.is_null() {
            return Err(HostError::Native(
                "no active content browser is available for DevTools".to_owned(),
            ));
        }

        unsafe {
            let get_host = (*browser)
                .get_host
                .ok_or_else(|| HostError::Native("CEF browser get_host unavailable".to_owned()))?;
            let browser_host = get_host(browser);
            if browser_host.is_null() {
                return Err(HostError::Native(
                    "CEF browser host unavailable for DevTools".to_owned(),
                ));
            }

            let has_dev_tools = (*browser_host).has_dev_tools.ok_or_else(|| {
                HostError::Native("CEF has_dev_tools callback unavailable".to_owned())
            })?;
            if has_dev_tools(browser_host) != 0 {
                let close_dev_tools = (*browser_host).close_dev_tools.ok_or_else(|| {
                    HostError::Native("CEF close_dev_tools callback unavailable".to_owned())
                })?;
                close_dev_tools(browser_host);
            } else {
                let show_dev_tools = (*browser_host).show_dev_tools.ok_or_else(|| {
                    HostError::Native("CEF show_dev_tools callback unavailable".to_owned())
                })?;
                show_dev_tools(
                    browser_host,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null(),
                );
            }
        }
        Ok(())
    }

    fn clear_content_view(&mut self, view_id: ContentViewId) -> Result<(), Self::Error> {
        let content_backend =
            self.content_views.get(&view_id).copied().ok_or_else(|| {
                HostError::Native(format!("content view not found: {}", view_id.0))
            })?;
        unsafe {
            match content_backend {
                ContentBackend::WebKit(view) => {
                    attach_wk_web_view(view, "about:blank")?;
                }
                ContentBackend::Cef(_) => {
                    let browser = self.browser_for_content_view(view_id)?;
                    let get_main_frame = (*browser).get_main_frame.ok_or_else(|| {
                        HostError::Native("CEF browser get_main_frame unavailable".to_owned())
                    })?;
                    let frame = get_main_frame(browser);
                    if frame.is_null() {
                        return Err(HostError::Native(
                            "CEF browser main frame unavailable".to_owned(),
                        ));
                    }
                    let load_url = (*frame).load_url.ok_or_else(|| {
                        HostError::Native("CEF frame load_url unavailable".to_owned())
                    })?;
                    with_stack_cef_string("about:blank", |url| load_url(frame, url));
                }
            }
        }
        set_active_content_tab(None);
        Ok(())
    }

    fn destroy_content_view(&mut self, view_id: ContentViewId) -> Result<(), Self::Error> {
        let tab_id = self.content_view_tabs.get(&view_id).copied();
        if let Some(tab_id) = tab_id {
            let browser = browser_for_tab(tab_id);
            if !browser.is_null() {
                unsafe {
                    if let Some(get_host) = (*browser).get_host {
                        let browser_host = get_host(browser);
                        if !browser_host.is_null() {
                            if let Some(close_browser) = (*browser_host).close_browser {
                                close_browser(browser_host, 1);
                            }
                        }
                    }
                }
            }
        }
        let content_backend = self
            .content_views
            .remove(&view_id)
            .ok_or_else(|| HostError::Native(format!("content view not found: {}", view_id.0)))?;
        let container = match content_backend {
            ContentBackend::WebKit(container) | ContentBackend::Cef(container) => container,
        };
        clear_active_content_container_if_matches(container)?;
        self.content_view_windows.remove(&view_id);
        self.content_view_tabs.remove(&view_id);

        unsafe {
            remove_all_subviews(container)?;
            msg_send_void(container, selector("removeFromSuperview")?);
        }

        if let Some(client) = self.cef_clients.remove(&view_id) {
            // CEF may still dispatch late callbacks while the browser tears down.
            // Defer freeing until full CEF shutdown to avoid use-after-free crashes.
            self.retired_cef_clients.push(client);
        }

        set_active_content_tab(None);
        set_active_content_browser(std::ptr::null_mut());
        Ok(())
    }

    fn has_live_content_browser(&self, tab_id: TabId) -> bool {
        !browser_for_tab(tab_id).is_null()
    }

    fn run_event_loop(&mut self) -> Result<(), Self::Error> {
        unsafe {
            msg_send_void_bool(self.app, selector("activateIgnoringOtherApps:")?, YES);
            if self.cef.is_some() {
                // Keep CEF runtime available while message loop is running so
                // UI-originated intents can navigate content views.
                let cef = self
                    .cef
                    .as_ref()
                    .ok_or_else(|| HostError::Native("CEF runtime unavailable".to_owned()))?;
                CEF_CLOSE_ALL_REQUESTED.store(false, Ordering::Release);
                cef.run_message_loop();
                let ui_client = cef.ui_client();
                for client in self.cef_clients.values().copied() {
                    free_content_cef_client(client);
                }
                for client in self.retired_cef_clients.drain(..) {
                    free_content_cef_client(client);
                }
                self.cef_clients.clear();
                cef.shutdown();
                free_ui_cef_client(ui_client);
                self.cef = None;
                set_active_content_tab(None);
                set_active_content_browser(std::ptr::null_mut());
                set_active_content_container(NIL)?;
                set_ui_shell_browser(std::ptr::null_mut());
            } else {
                msg_send_void(self.app, selector("run")?);
            }
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
unsafe fn objc_class(name: &str) -> Result<ObjcId, HostError> {
    let name = CString::new(name)
        .map_err(|_| HostError::Native("objc class name had interior NUL".to_owned()))?;
    let class = objc_getClass(name.as_ptr());
    if class == NIL {
        return Err(HostError::Native("objc class lookup failed".to_owned()));
    }
    Ok(class)
}

#[cfg(target_os = "macos")]
unsafe fn selector(name: &str) -> Result<ObjcSel, HostError> {
    let name = CString::new(name)
        .map_err(|_| HostError::Native("objc selector had interior NUL".to_owned()))?;
    let sel = sel_registerName(name.as_ptr());
    if sel == NIL {
        return Err(HostError::Native("objc selector lookup failed".to_owned()));
    }
    Ok(sel)
}

#[cfg(target_os = "macos")]
unsafe fn nsstring(value: &str) -> Result<ObjcId, HostError> {
    let string_class = objc_class("NSString")?;
    let alloc = msg_send_id(string_class, selector("alloc")?);
    let utf8 = CString::new(value)
        .map_err(|_| HostError::Native("NSString value had interior NUL".to_owned()))?;
    let string_value = msg_send_id_cstr(alloc, selector("initWithUTF8String:")?, utf8.as_ptr());
    if string_value == NIL {
        return Err(HostError::Native("failed to create NSString".to_owned()));
    }
    Ok(string_value)
}

#[cfg(target_os = "macos")]
unsafe fn content_container_frame(root_view: ObjcId) -> Result<NSRect, HostError> {
    let bounds = msg_send_rect(root_view, selector("bounds")?);
    let width = (bounds.size.width - UI_LEFT_WIDTH).max(1.0);
    let height = (bounds.size.height - UI_TOP_HEIGHT).max(1.0);
    Ok(NSRect {
        origin: NSPoint {
            x: UI_LEFT_WIDTH,
            y: 0.0,
        },
        size: NSSize { width, height },
    })
}

#[cfg(target_os = "macos")]
unsafe fn create_content_container(frame: NSRect) -> Result<ObjcId, HostError> {
    let view_class = objc_class("NSView")?;
    let view_alloc = msg_send_id(view_class, selector("alloc")?);
    let content_view = msg_send_id_rect(view_alloc, selector("initWithFrame:")?, frame);
    if content_view == NIL {
        return Err(HostError::Native(
            "failed to create content container view".to_owned(),
        ));
    }
    Ok(content_view)
}

#[cfg(target_os = "macos")]
unsafe fn attach_wk_web_view(container: ObjcId, url: &str) -> Result<(), HostError> {
    remove_all_subviews(container)?;
    let (width, height) = current_view_size(container)?;

    let config_class = objc_class("WKWebViewConfiguration")?;
    let config_alloc = msg_send_id(config_class, selector("alloc")?);
    let web_config = msg_send_id(config_alloc, selector("init")?);
    if web_config == NIL {
        return Err(HostError::Native(
            "failed to create WKWebViewConfiguration".to_owned(),
        ));
    }
    let data_store_class = objc_class("WKWebsiteDataStore")?;
    let data_store = msg_send_id(data_store_class, selector("nonPersistentDataStore")?);
    if data_store != NIL {
        msg_send_void_id(web_config, selector("setWebsiteDataStore:")?, data_store);
    }

    let web_view_class = objc_class("WKWebView")?;
    let web_view_alloc = msg_send_id(web_view_class, selector("alloc")?);
    let frame = NSRect {
        origin: NSPoint { x: 0.0, y: 0.0 },
        size: NSSize { width, height },
    };
    let web_view = msg_send_id_rect_id(
        web_view_alloc,
        selector("initWithFrame:configuration:")?,
        frame,
        web_config,
    );
    if web_view == NIL {
        return Err(HostError::Native("failed to create WKWebView".to_owned()));
    }

    msg_send_void_id(container, selector("addSubview:")?, web_view);
    load_url_in_web_view(web_view, url)
}

#[cfg(target_os = "macos")]
unsafe fn remove_all_subviews(view: ObjcId) -> Result<(), HostError> {
    let subviews = msg_send_id(view, selector("subviews")?);
    if subviews == NIL {
        return Ok(());
    }

    // NSView.subviews can be returned as an immutable snapshot array. Iterate that
    // snapshot once instead of polling count in a loop, which can otherwise hang.
    let count = msg_send_usize(subviews, selector("count")?);
    for index in (0..count).rev() {
        let child = msg_send_id_usize(subviews, selector("objectAtIndex:")?, index);
        if child == NIL {
            continue;
        }
        msg_send_void(child, selector("removeFromSuperview")?);
    }

    Ok(())
}

#[cfg(target_os = "macos")]
unsafe fn current_view_size(view: ObjcId) -> Result<(f64, f64), HostError> {
    let bounds = msg_send_rect(view, selector("bounds")?);
    let width = bounds.size.width.max(1.0);
    let height = bounds.size.height.max(1.0);
    Ok((width, height))
}

#[cfg(target_os = "macos")]
unsafe fn load_url_in_web_view(web_view: ObjcId, url: &str) -> Result<(), HostError> {
    let url_string = nsstring(url)?;
    let nsurl_class = objc_class("NSURL")?;
    let nsurl = msg_send_id_id(nsurl_class, selector("URLWithString:")?, url_string);
    if nsurl == NIL {
        return Err(HostError::Native(format!("failed to parse URL: {url}")));
    }

    let request_class = objc_class("NSURLRequest")?;
    let request = msg_send_id_id(request_class, selector("requestWithURL:")?, nsurl);
    if request == NIL {
        return Err(HostError::Native(
            "failed to create NSURLRequest".to_owned(),
        ));
    }

    let _ = msg_send_id_id(web_view, selector("loadRequest:")?, request);
    Ok(())
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_id(receiver: ObjcId, selector: ObjcSel) -> ObjcId {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel) -> ObjcId =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector)
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_bool(receiver: ObjcId, selector: ObjcSel) -> i8 {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel) -> i8 =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector)
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_id_cstr(receiver: ObjcId, selector: ObjcSel, arg: *const c_char) -> ObjcId {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel, *const c_char) -> ObjcId =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, arg)
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_id_rect(receiver: ObjcId, selector: ObjcSel, rect: NSRect) -> ObjcId {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel, NSRect) -> ObjcId =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, rect)
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_id_rect_id(
    receiver: ObjcId,
    selector: ObjcSel,
    rect: NSRect,
    arg: ObjcId,
) -> ObjcId {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel, NSRect, ObjcId) -> ObjcId =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, rect, arg)
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_id_rect_u64_u64_bool(
    receiver: ObjcId,
    selector: ObjcSel,
    rect: NSRect,
    style: u64,
    backing: u64,
    should_defer: i8,
) -> ObjcId {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel, NSRect, u64, u64, i8) -> ObjcId =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, rect, style, backing, should_defer)
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_id_id(receiver: ObjcId, selector: ObjcSel, arg: ObjcId) -> ObjcId {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel, ObjcId) -> ObjcId =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, arg)
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_id_usize(receiver: ObjcId, selector: ObjcSel, arg: usize) -> ObjcId {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel, usize) -> ObjcId =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, arg)
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_rect(receiver: ObjcId, selector: ObjcSel) -> NSRect {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel) -> NSRect =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector)
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_usize(receiver: ObjcId, selector: ObjcSel) -> usize {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel) -> usize =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector)
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_void(receiver: ObjcId, selector: ObjcSel) {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel) =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector);
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_void_id(receiver: ObjcId, selector: ObjcSel, arg: ObjcId) {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel, ObjcId) =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, arg);
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_void_rect(receiver: ObjcId, selector: ObjcSel, rect: NSRect) {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel, NSRect) =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, rect);
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_void_bool(receiver: ObjcId, selector: ObjcSel, arg: i8) {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel, i8) =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, arg);
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_void_i64(receiver: ObjcId, selector: ObjcSel, arg: i64) {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel, i64) =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, arg);
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_void_u64(receiver: ObjcId, selector: ObjcSel, arg: u64) {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel, u64) =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, arg);
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_void_f64(receiver: ObjcId, selector: ObjcSel, arg: f64) {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel, f64) =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, arg);
}

#[cfg(target_os = "macos")]
unsafe fn msg_send_void_id_i64_id(
    receiver: ObjcId,
    selector: ObjcSel,
    view: ObjcId,
    ordering_mode: i64,
    relative_to: ObjcId,
) {
    let send: unsafe extern "C" fn(ObjcId, ObjcSel, ObjcId, i64, ObjcId) =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, view, ordering_mode, relative_to);
}

#[cfg(not(target_os = "macos"))]
#[derive(Debug, Default)]
pub struct NativeMacHost;

#[cfg(not(target_os = "macos"))]
impl NativeMacHost {
    pub fn new() -> Result<Self, HostError> {
        Err(HostError::UnsupportedPlatform)
    }
}
