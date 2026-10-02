//! End-to-end test: the real keyboard hook, real key events, a real window.
//!
//!     cargo run -p ac-e2e --release
//!
//! Opens a small window with an edit box, installs the same hook as
//! `autocorrect.exe`, types into the box with real key events (SendInput) and
//! compares the text that ends up in it. Nothing is typed anywhere else: the
//! window must be in front before every key, otherwise the run stops. It
//! refuses to start while AutoCorrect itself is running (two hooks would both
//! rewrite the keys). Do not touch the keyboard while it runs (about 15 s).
//!
//! The scenario that matters most reproduces the "toôi" bug: the edit box
//! completes folder names inline (like a browser address bar), the completed
//! part is selected, and the first Backspace used to delete that instead of
//! the letter just typed.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use ac_platform_win::settings::Settings;
use ac_platform_win::Personal;
use ac_platform_win::{hook, log};
use windows::core::{w, Result};
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Ole::OleInitialize;
use windows::Win32::System::Threading::{CreateMutexW, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, SendInput, SetFocus, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    MAPVK_VK_TO_VSC, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_MENU, VK_SHIFT, VK_SPACE,
};
use windows::Win32::UI::Shell::{SHAutoComplete, SHACF_AUTOAPPEND_FORCE_ON, SHACF_FILESYS_DIRS};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetForegroundWindow, GetMessageW,
    PostThreadMessageW, RegisterClassW, SendMessageW, SetForegroundWindow, SetWindowsHookExW, ShowWindow,
    TranslateMessage, UnhookWindowsHookEx, MSG, SW_SHOW, WH_KEYBOARD_LL, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_APP, WM_GETTEXT, WM_GETTEXTLENGTH, WM_QUIT, WM_SETFOCUS, WM_SETTEXT, WNDCLASSW, WS_CHILD,
    WS_EX_CLIENTEDGE, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

const WM_JOB: u32 = WM_APP + 9;
/// Edit control message: select a character range (here: place the caret).
const EM_SETSEL: u32 = 0x00B1;
static JOBS: Mutex<VecDeque<Box<dyn FnOnce() + Send>>> = Mutex::new(VecDeque::new());
/// The edit box, so that the top-level window can hand the keyboard focus on.
static EDIT: AtomicIsize = AtomicIsize::new(0);

/// What the driver thread controls.
#[derive(Clone, Copy)]
struct Window {
    top: isize,
    edit: isize,
    main_thread: u32,
}

impl Window {
    fn top(&self) -> HWND {
        HWND(self.top as _)
    }

    fn edit(&self) -> HWND {
        HWND(self.edit as _)
    }

    /// Runs `f` on the thread that owns the window and the hook, and waits.
    fn on_main(&self, f: impl FnOnce() + Send + 'static) {
        let (done, wait) = mpsc::channel();
        JOBS.lock().expect("jobs").push_back(Box::new(move || {
            f();
            let _ = done.send(());
        }));
        unsafe {
            let _ = PostThreadMessageW(self.main_thread, WM_JOB, WPARAM(0), LPARAM(0));
        }
        let _ = wait.recv_timeout(Duration::from_secs(5));
    }

    fn text(&self) -> String {
        unsafe {
            let len = SendMessageW(self.edit(), WM_GETTEXTLENGTH, None, None).0 as usize;
            let mut buf = vec![0u16; len + 1];
            SendMessageW(self.edit(), WM_GETTEXT, Some(WPARAM(buf.len())), Some(LPARAM(buf.as_mut_ptr() as isize)));
            String::from_utf16_lossy(&buf[..len])
        }
    }

    /// Replaces the text and puts the caret at its end.
    fn set_text(&self, text: &str) {
        let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
        unsafe {
            SendMessageW(self.edit(), WM_SETTEXT, None, Some(LPARAM(wide.as_ptr() as isize)));
            let end = text.encode_utf16().count();
            SendMessageW(self.edit(), EM_SETSEL, Some(WPARAM(end)), Some(LPARAM(end as isize)));
        }
    }

    /// The text once it has stopped changing for 300 ms. Keys typed faster
    /// than the replacements are made queue up and are still being worked off
    /// after the last one was sent.
    fn settled_text(&self) -> String {
        let mut last = self.text();
        let mut stable = 0;
        for _ in 0..80 {
            thread::sleep(Duration::from_millis(50));
            let now = self.text();
            if now == last {
                stable += 1;
                if stable >= 6 {
                    break;
                }
            } else {
                stable = 0;
                last = now;
            }
        }
        last
    }

    fn in_front(&self) -> bool {
        unsafe { GetForegroundWindow() == self.top() }
    }

    /// One key press, with modifiers. Refuses if the window lost the front.
    fn press(&self, vk: u16, shift: bool, ctrl: bool) -> std::result::Result<(), String> {
        if !self.in_front() {
            return Err("the test window is no longer in front: stopped before typing anywhere else".into());
        }
        let key = |vk: u16, up: bool| {
            let scan = unsafe { MapVirtualKeyW(u32::from(vk), MAPVK_VK_TO_VSC) } as u16;
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(vk),
                        wScan: scan,
                        dwFlags: if up { KEYEVENTF_KEYUP } else { Default::default() },
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            }
        };
        let mut inputs = Vec::new();
        if ctrl {
            inputs.push(key(VK_CONTROL.0, false));
        }
        if shift {
            inputs.push(key(VK_SHIFT.0, false));
        }
        inputs.push(key(vk, false));
        inputs.push(key(vk, true));
        if shift {
            inputs.push(key(VK_SHIFT.0, true));
        }
        if ctrl {
            inputs.push(key(VK_CONTROL.0, true));
        }
        unsafe {
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }
        Ok(())
    }

    fn type_text(&self, text: &str, gap_ms: u64) -> std::result::Result<(), String> {
        for c in text.chars() {
            let (vk, shift) = key_for(c).ok_or_else(|| format!("cannot type {c:?}"))?;
            self.press(vk, shift, false)?;
            thread::sleep(Duration::from_millis(gap_ms));
        }
        Ok(())
    }
}

/// US layout: the virtual key for a character, and whether Shift is held.
fn key_for(c: char) -> Option<(u16, bool)> {
    Some(match c {
        'a'..='z' => (c.to_ascii_uppercase() as u16, false),
        'A'..='Z' => (c as u16, true),
        '0'..='9' => (c as u16, false),
        ' ' => (VK_SPACE.0, false),
        ',' => (0xBC, false),
        '.' => (0xBE, false),
        ';' => (0xBA, false),
        ':' => (0xBA, true),
        '?' => (0xBF, true),
        '!' => (0x31, true),
        '-' => (0xBD, false),
        '_' => (0xBD, true),
        '\\' => (0xDC, false),
        _ => return None,
    })
}

enum Step {
    /// Type text with this gap (ms) between keys.
    Text(String, u64),
    Backspace,
    CtrlZ,
    Wait(u64),
}

struct Scenario {
    name: &'static str,
    settings: Settings,
    /// Edit box text before typing (the caret goes to its end).
    start: String,
    steps: Vec<Step>,
    expect: String,
    /// Contents of the personal dictionary for this scenario.
    personal: &'static str,
}

fn settings(vietnamese: bool) -> Settings {
    Settings {
        vietnamese,
        corrections: true,
        paused: false,
        journal: false,
        code_english: false,
        autocomplete_guard: true,
        restore_marks: true,
    }
}

fn run(window: &Window, s: &Scenario) -> std::result::Result<String, String> {
    let settings = s.settings;
    let personal = s.personal;
    let edit = window.edit;
    window.on_main(move || {
        // Typed keys go to the focused window: make sure that is the edit box.
        unsafe {
            let _ = SetFocus(Some(HWND(edit as _)));
        }
        hook::update(|current| *current = settings);
        hook::set_personal(Personal::parse(personal));
        hook::reset_engine();
    });
    window.set_text(&s.start);
    thread::sleep(Duration::from_millis(100));
    for step in &s.steps {
        match step {
            Step::Text(text, gap) => window.type_text(text, *gap)?,
            Step::Backspace => window.press(VK_BACK.0, false, false)?,
            Step::CtrlZ => window.press(u16::from(b'Z'), false, true)?,
            Step::Wait(ms) => thread::sleep(Duration::from_millis(*ms)),
        }
        thread::sleep(Duration::from_millis(20));
    }
    Ok(window.settled_text())
}

fn text(t: &str, gap: u64) -> Step {
    Step::Text(t.to_string(), gap)
}

/// The steps of the inline-completion scenario: "to" is completed to "tools"
/// with "ols" selected; the next "o" makes "ô" and needs the Backspace to hit
/// the typed "o".
fn completion_steps() -> Vec<Step> {
    vec![text("to", 40), Step::Wait(900), text("o", 40), Step::Wait(300)]
}

fn scenarios(folder: &str) -> Vec<Scenario> {
    let plain = |name, vi, steps, expect: &str| Scenario {
        name,
        settings: settings(vi),
        start: String::new(),
        steps,
        expect: expect.to_string(),
        personal: "",
    };
    let sep = std::path::MAIN_SEPARATOR;
    vec![
        plain("English: a typo is fixed at Space", false, vec![text("teh ", 30)], "the "),
        plain("English: punctuation ends the word", false, vec![text("teh, recieve.", 30)], "the, receive."),
        plain("Telex: marks and tones", true, vec![text("tooi vieetj ", 30)], "tôi việt "),
        plain("Telex typed fast (2 ms between keys)", true, vec![text("tooi laf ai ddaay laf ddaau ", 2)], "tôi là ai đây là đâu "),
        plain("Telex: a mistyped word is fixed", true, vec![text("dunhf ", 30)], "dùng "),
        plain("Telex: words typed without marks", true, vec![text("tooi khong ", 30)], "tôi không "),
        plain("Telex: an ambiguous bare word is left alone", true, vec![text("ban ", 30)], "ban "),
        plain("Ctrl+Z undoes the fix and keeps the space", false, vec![text("teh ", 30), Step::CtrlZ], "teh "),
        // Not ours to take: the edit box undoes its own typing, which proves
        // that the key reached it.
        plain("Ctrl+Z with nothing to undo is the program's own", false, vec![text("abc", 30), Step::CtrlZ], ""),
        plain("Backspace only edits (no undo)", false, vec![text("teh ", 30), Step::Backspace, text("m", 30)], "them"),
        plain("Several Backspaces keep editing", false, vec![text("teh ", 30), Step::Backspace, Step::Backspace], "th"),
        Scenario {
            personal: "fix\tko\tkhông",
            ..plain("Personal dictionary: a fix replaces what was typed", false, vec![text("ko ", 30)], "không ")
        },
        Scenario {
            personal: "ignore\tteh",
            ..plain("Personal dictionary: an ignored word is left alone", false, vec![text("teh ", 30)], "teh ")
        },
        Scenario {
            name: "Inline completion, guard on: tôi, not toôi",
            settings: settings(true),
            start: format!("{folder}{sep}"),
            steps: completion_steps(),
            expect: format!("{folder}{sep}tô"),
            personal: "",
        },
    ]
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_SETFOCUS {
        // A plain window keeps the focus for itself: pass it to the edit box
        // (the first keys of the first scenario were lost to this).
        let edit = EDIT.load(Ordering::Relaxed);
        if edit != 0 {
            let _ = SetFocus(Some(HWND(edit as _)));
            return LRESULT(0);
        }
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn main() -> Result<()> {
    unsafe {
        // A second hook (the real app) would rewrite every key twice.
        let _single = CreateMutexW(None, true, w!("Local\\AutoCorrect.SingleInstance"))?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            eprintln!("AutoCorrect is running: quit it from its tray icon, then run this again.");
            std::process::exit(2);
        }
        let _ = OleInitialize(None);
    }
    log::start(false);

    // A folder named "tools" for the inline-completion scenario.
    let root: PathBuf = std::env::temp_dir().join(format!("ac-e2e-{}", std::process::id()));
    std::fs::create_dir_all(root.join("tools")).expect("create test folder");
    let folder = root.display().to_string();

    let (window, hook_handle) = unsafe {
        let instance = GetModuleHandleW(None)?.into();
        let class = w!("AcE2eWindow");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(window_proc), hInstance: instance, lpszClassName: class, ..Default::default() });
        let top = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("ac-e2e: do not type"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            100,
            100,
            760,
            120,
            None,
            None,
            Some(instance),
            None,
        )?;
        let edit = CreateWindowExW(
            WS_EX_CLIENTEDGE,
            w!("EDIT"),
            w!(""),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(0x80), // ES_AUTOHSCROLL
            10,
            10,
            720,
            28,
            Some(top),
            None,
            Some(instance),
            None,
        )?;
        EDIT.store(edit.0 as isize, Ordering::Relaxed);
        // Inline completion of folder names, like a browser address bar.
        let _ = SHAutoComplete(edit, SHACF_FILESYS_DIRS | SHACF_AUTOAPPEND_FORCE_ON);

        // Windows only lets the foreground process change the foreground;
        // a tap of Alt (before our hook exists) lifts that lock.
        let tap = |up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_MENU,
                    wScan: 0,
                    dwFlags: if up { KEYEVENTF_KEYUP } else { Default::default() },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        SendInput(&[tap(false), tap(true)], std::mem::size_of::<INPUT>() as i32);
        let _ = ShowWindow(top, SW_SHOW);
        let _ = BringWindowToTop(top);
        let _ = SetForegroundWindow(top);
        let _ = SetFocus(Some(edit));

        hook::init(settings(true));
        let handle = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook::keyboard_proc), Some(instance), 0)?;
        (Window { top: top.0 as isize, edit: edit.0 as isize, main_thread: GetCurrentThreadId() }, handle)
    };

    let list = scenarios(&folder);
    let driver = thread::spawn(move || {
        thread::sleep(Duration::from_millis(800));
        // Warm-up: the first key into a fresh edit box with inline completion
        // was sometimes swallowed by the control itself (the hook had seen it).
        let _ = window.type_text("x", 40);
        thread::sleep(Duration::from_millis(600));
        window.set_text("");
        let mut failures = 0;
        println!("{:<52} result", "scenario");
        for s in &list {
            match run(&window, s) {
                Ok(got) if got == s.expect => println!("{:<52} ok", s.name),
                Ok(got) => {
                    failures += 1;
                    println!("{:<52} FAIL\n    expected {:?}\n    got      {:?}", s.name, s.expect, got);
                }
                Err(why) => {
                    failures += 1;
                    println!("{:<52} STOPPED: {why}", s.name);
                    break;
                }
            }
        }

        // The same scenario without the guard: what the guard is there for.
        if let Some(guarded) = list.last() {
            let mut settings = guarded.settings;
            settings.autocomplete_guard = false;
            let unguarded = Scenario {
                name: "",
                settings,
                start: guarded.start.clone(),
                steps: completion_steps(),
                expect: guarded.expect.clone(),
                personal: "",
            };
            let label = "Inline completion, guard OFF (informational)";
            match run(&window, &unguarded) {
                Ok(got) if got == unguarded.expect => println!("{label:<52} no completion appeared: inconclusive"),
                Ok(got) => println!("{label:<52} bug reproduced without the guard: {got:?}"),
                Err(why) => println!("{label:<52} STOPPED: {why}"),
            }
        }
        println!("\n{}", if failures == 0 { "all scenarios passed".to_string() } else { format!("{failures} scenario(s) failed") });
        unsafe {
            let _ = PostThreadMessageW(window.main_thread, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        failures
    });

    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_JOB {
                loop {
                    let job = JOBS.lock().expect("jobs").pop_front();
                    match job {
                        Some(job) => job(),
                        None => break,
                    }
                }
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = UnhookWindowsHookEx(hook_handle);
    }
    let failures = driver.join().unwrap_or(1);
    let _ = std::fs::remove_dir_all(&root);
    std::process::exit(if failures == 0 { 0 } else { 1 });
}
