use serde::Serialize;
use std::io::{self, BufRead, Write};
use std::sync::{atomic::{AtomicBool, Ordering}, Mutex};
use std::ptr;
use std::thread;
use std::time::Duration;

#[cfg(windows)]
use windows::Win32::System::Console::FreeConsole;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM, HINSTANCE};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, HHOOK, KBDLLHOOKSTRUCT, MSG,
    PeekMessageW, PM_REMOVE, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

#[derive(Serialize)]
struct KeyEvent {
    r#type: &'static str,
    event: &'static str,
    code: u32,
}

fn send_event(event_type: &'static str, vk_code: u32) {
    let evt = KeyEvent {
        r#type: event_type,
        event: if event_type == "key" { "down" } else { "up" },
        code: vk_code,
    };
    // 🔧 Формируем JSON вручную для скорости и надёжности
    let json = format!(r#"{{"type":"{}","event":"{}","code":{}}}"#, 
        evt.r#type, evt.event, evt.code);
    
    // 🔧 Пишем в stdout и сразу сбрасываем буфер (критично для труб!)
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    let _ = writeln!(handle, "{}", json);
    let _ = handle.flush();
}

static RUNNING: AtomicBool = AtomicBool::new(false);
static HOOK_HANDLE: Mutex<Option<HHOOK>> = Mutex::new(None);

extern "system" fn keyboard_hook_proc(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if n_code < 0 {
        return unsafe { CallNextHookEx(HHOOK(ptr::null_mut()), n_code, w_param, l_param) };
    }

    let kb = unsafe { &*(l_param.0 as *const KBDLLHOOKSTRUCT) };
    let vk = kb.vkCode;
    
    match w_param.0 as u32 {
        code if code == WM_KEYDOWN || code == WM_SYSKEYDOWN => {
            send_event("key", vk);
        }
        code if code == WM_KEYUP || code == WM_SYSKEYUP => {
            // Можно отправлять up-события, если нужно
            // send_event("key_up", vk);
        }
        _ => {}
    }

    let hook_guard = HOOK_HANDLE.lock().unwrap();
    if let Some(hook) = *hook_guard {
        unsafe { CallNextHookEx(hook, n_code, w_param, l_param) }
    } else {
        LRESULT(0)
    }
}

fn install_hook() -> Result<HHOOK, String> {
    unsafe {
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), HINSTANCE(ptr::null_mut()), 0)
    }.map_err(|e| format!("SetWindowsHookExW failed: {}", e))
}

fn message_loop() {
    let mut msg = MSG::default();
    while RUNNING.load(Ordering::SeqCst) {
        let has_msg = unsafe { PeekMessageW(&mut msg, HWND(ptr::null_mut()), 0, 0, PM_REMOVE) };
        if has_msg.as_bool() {
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        } else {
            // Небольшая пауза чтобы не грузить CPU
            thread::sleep(Duration::from_millis(2));
        }
    }
}

fn cleanup_hook() {
    if let Ok(mut guard) = HOOK_HANDLE.lock() {
        if let Some(hook) = guard.take() {
            unsafe { let _ = UnhookWindowsHookEx(hook); }
        }
    }
}

fn main() {
    // 🔧 На Windows: скрываем консольное окно, если приложение запущено как GUI
    #[cfg(windows)]
    unsafe { let _ = FreeConsole(); }

    // 🔧 Включаем буферизацию stdout для надёжной работы с трубами
    let stdout = io::stdout();
    let _ = stdout.lock().flush();

    // Отправляем сигнал готовности
    let _ = writeln!(io::stdout(), r#"{"type":"init","status":"ready"}"#);
    let _ = io::stdout().flush();

    RUNNING.store(true, Ordering::SeqCst);

    // Устанавливаем хук
    let hook = match install_hook() {
        Ok(h) => {
            *HOOK_HANDLE.lock().unwrap() = Some(h);
            h
        }
        Err(e) => {
            let _ = writeln!(io::stderr(), r#"{{"type":"error","message":"{}"}}"#, e);
            let _ = io::stderr().flush();
            return;
        }
    };

    // 🔧 Читаем команды с stdin (например, "stop" для завершения)
    let stdin_handle = thread::spawn(|| {
        let stdin = io::stdin();
        for line in stdin.lock().lines().flatten() {
            if line.trim().eq_ignore_ascii_case("stop") {
                RUNNING.store(false, Ordering::SeqCst);
                break;
            }
        }
    });

    // Message loop для обработки хука
    message_loop();

    // Ждём завершения потока stdin
    let _ = stdin_handle.join();

    // Cleanup
    cleanup_hook();
    
    let _ = writeln!(io::stdout(), r#"{"type":"exit","status":"clean"}"#);
    let _ = io::stdout().flush();
}