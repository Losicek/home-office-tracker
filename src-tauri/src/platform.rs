//! Systémově závislé dotazy: jaká aplikace je právě v popředí a jak dlouho
//! uživatel nesáhl na klávesnici/myš. Záměrně jen název aplikace — žádné
//! titulky oken, takže na macOS nejsou potřeba oprávnění Zpřístupnění ani
//! Záznam obrazovky.

#[cfg(target_os = "macos")]
mod imp {
    use objc2_app_kit::NSWorkspace;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64;
    }

    const HID_SYSTEM_STATE: i32 = 1;
    const ANY_INPUT_EVENT: u32 = u32::MAX;

    pub fn idle_seconds() -> u64 {
        let secs =
            unsafe { CGEventSourceSecondsSinceLastEventType(HID_SYSTEM_STATE, ANY_INPUT_EVENT) };
        if secs.is_finite() && secs > 0.0 {
            secs as u64
        } else {
            0
        }
    }

    pub fn frontmost_app() -> Option<String> {
        let workspace = NSWorkspace::sharedWorkspace();
        let app = workspace.frontmostApplication()?;
        app.localizedName().map(|name| name.to_string())
    }

    /// Kontejner iCloudu appky, nebo `None` (nepřihlášený iCloud, vypnutý
    /// iCloud Drive, nepodepsaná appka bez entitlementů). Může blokovat —
    /// nevolat z hlavního vlákna.
    pub fn icloud_container() -> Option<std::path::PathBuf> {
        use objc2_foundation::{NSFileManager, NSString};
        let id = NSString::from_str(super::ICLOUD_CONTAINER);
        let url = NSFileManager::defaultManager().URLForUbiquityContainerIdentifier(Some(&id))?;
        url.path().map(|p| std::path::PathBuf::from(p.to_string()))
    }

    /// Soubor, který iCloud drží jen v cloudu (`.jmeno.icloud`), si vyžádá
    /// ke stažení; při dalším průchodu už bude na disku.
    pub fn request_download(path: &std::path::Path) {
        use objc2_foundation::{NSFileManager, NSString, NSURL};
        let path = NSString::from_str(&path.to_string_lossy());
        let url = NSURL::fileURLWithPath(&path);
        let _ = NSFileManager::defaultManager().startDownloadingUbiquitousItemAtURL_error(&url);
    }

    #[link(name = "SystemConfiguration", kind = "framework")]
    extern "C" {
        fn SCDynamicStoreCopyComputerName(
            store: *const std::ffi::c_void,
            encoding: *mut u32,
        ) -> *const std::ffi::c_void;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringGetCString(
            s: *const std::ffi::c_void,
            buf: *mut std::ffi::c_char,
            size: isize,
            encoding: u32,
        ) -> u8;
        fn CFRelease(cf: *const std::ffi::c_void);
    }

    /// Název počítače ze Nastavení systému („MacBook Pro uživatele …“).
    pub fn device_name() -> String {
        const UTF8: u32 = 0x0800_0100;
        unsafe {
            let name = SCDynamicStoreCopyComputerName(std::ptr::null(), std::ptr::null_mut());
            if name.is_null() {
                return "Mac".into();
            }
            let mut buf = [0 as std::ffi::c_char; 256];
            let ok = CFStringGetCString(name, buf.as_mut_ptr(), buf.len() as isize, UTF8);
            CFRelease(name);
            if ok == 0 {
                return "Mac".into();
            }
            std::ffi::CStr::from_ptr(buf.as_ptr())
                .to_string_lossy()
                .into_owned()
        }
    }
}

#[cfg(windows)]
mod imp {
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    pub fn idle_seconds() -> u64 {
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        unsafe {
            if !GetLastInputInfo(&mut info).as_bool() {
                return 0;
            }
            (GetTickCount().wrapping_sub(info.dwTime) / 1000) as u64
        }
    }

    pub fn frontmost_app() -> Option<String> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 {
                return None;
            }
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                PWSTR(buf.as_mut_ptr()),
                &mut len,
            );
            let _ = CloseHandle(process);
            ok.ok()?;
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            Some(file_description(&buf[..len as usize]).unwrap_or_else(|| exe_stem(&path)))
        }
    }

    pub fn icloud_container() -> Option<std::path::PathBuf> {
        None
    }

    pub fn request_download(_path: &std::path::Path) {}

    pub fn device_name() -> String {
        std::env::var("COMPUTERNAME").unwrap_or_else(|_| "PC".into())
    }

    fn exe_stem(path: &str) -> String {
        std::path::Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string())
    }

    /// Čitelný název z metadat .exe („Microsoft Excel“ místo „EXCEL“).
    unsafe fn file_description(path: &[u16]) -> Option<String> {
        let mut wide: Vec<u16> = path.to_vec();
        wide.push(0);
        let path = PCWSTR(wide.as_ptr());
        let size = GetFileVersionInfoSizeW(path, None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(path, None, size, data.as_mut_ptr().cast()).ok()?;

        // První jazyk/kódová stránka z tabulky překladů.
        let mut ptr = std::ptr::null_mut();
        let mut len = 0u32;
        let key: Vec<u16> = "\\VarFileInfo\\Translation\0".encode_utf16().collect();
        if !VerQueryValueW(
            data.as_ptr().cast(),
            PCWSTR(key.as_ptr()),
            &mut ptr,
            &mut len,
        )
        .as_bool()
            || len < 4
        {
            return None;
        }
        let lang = *(ptr as *const u16);
        let codepage = *(ptr as *const u16).add(1);
        let key: Vec<u16> = format!(
            "\\StringFileInfo\\{:04x}{:04x}\\FileDescription\0",
            lang, codepage
        )
        .encode_utf16()
        .collect();
        if !VerQueryValueW(
            data.as_ptr().cast(),
            PCWSTR(key.as_ptr()),
            &mut ptr,
            &mut len,
        )
        .as_bool()
            || len == 0
        {
            return None;
        }
        let slice = std::slice::from_raw_parts(ptr as *const u16, len as usize);
        let text = String::from_utf16_lossy(slice)
            .trim_end_matches('\0')
            .trim()
            .to_string();
        (!text.is_empty()).then_some(text)
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod imp {
    pub fn idle_seconds() -> u64 {
        0
    }
    pub fn frontmost_app() -> Option<String> {
        None
    }
    pub fn icloud_container() -> Option<std::path::PathBuf> {
        None
    }
    pub fn request_download(_path: &std::path::Path) {}
    pub fn device_name() -> String {
        "PC".into()
    }
}

pub use imp::{device_name, frontmost_app, icloud_container, idle_seconds, request_download};

/// Musí odpovídat iCloud kontejneru v Apple Developer účtu a entitlementům.
pub const ICLOUD_CONTAINER: &str = "iCloud.com.losicek.homeofficetracker";
