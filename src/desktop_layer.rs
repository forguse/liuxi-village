//! Keep the village below application windows without changing keyboard focus.
use std::mem;
use windows_sys::Win32::{
    Foundation::{BOOL, HWND, LPARAM},
    Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute},
    UI::WindowsAndMessaging::*,
};

fn is_topmost(hwnd: HWND) -> bool {
    unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST != 0 }
}

fn is_desktop(hwnd: HWND) -> bool {
    if hwnd == unsafe { GetShellWindow() } {
        return true;
    }
    let mut class = [0u16; 96];
    let length = unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32) };
    if length <= 0 {
        return false;
    }
    let name = &class[..length as usize];
    ["Progman", "WorkerW", "SHELLDLL_DefView", "SysListView32"]
        .into_iter()
        .any(|class| name.iter().copied().eq(class.encode_utf16()))
        || name
            .get(.."PecoFence".len())
            .is_some_and(|prefix| prefix.iter().copied().eq("PecoFence".encode_utf16()))
}

fn owned_by(mut hwnd: HWND, owner: HWND) -> bool {
    for _ in 0..32 {
        if hwnd == owner {
            return true;
        }
        hwnd = unsafe { GetWindow(hwnd, GW_OWNER) };
        if hwnd.is_null() {
            break;
        }
    }
    false
}

struct Placement {
    home: HWND,
    after: HWND,
}

unsafe extern "system" fn inspect_window(hwnd: HWND, parameter: LPARAM) -> BOOL {
    let placement = unsafe { &mut *(parameter as *mut Placement) };
    if unsafe { IsWindowVisible(hwnd) } == 0
        || unsafe { IsIconic(hwnd) } != 0
        || is_topmost(hwnd)
        || owned_by(hwnd, placement.home)
        || is_desktop(hwnd)
    {
        return 1;
    }
    let mut cloaked = 0u32;
    unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED as u32,
            (&mut cloaked as *mut u32).cast(),
            mem::size_of_val(&cloaked) as u32,
        );
    }
    if cloaked == 0 {
        placement.after = hwnd;
    }
    1
}

pub(super) fn place(hwnd: HWND, show: bool) {
    let flags = SWP_NOACTIVATE
        | SWP_NOOWNERZORDER
        | SWP_NOMOVE
        | SWP_NOSIZE
        | if show { SWP_SHOWWINDOW } else { 0 };
    unsafe {
        // Clear legacy topmost state before positioning the village.
        if is_topmost(hwnd) {
            SetWindowPos(hwnd, HWND_BOTTOM, 0, 0, 0, 0, flags);
        }
        let mut placement = Placement {
            home: hwnd,
            after: HWND_TOP,
        };
        if EnumWindows(
            Some(inspect_window),
            (&mut placement as *mut Placement) as LPARAM,
        ) == 0
        {
            placement.after = HWND_BOTTOM;
        }
        // A window may disappear or become topmost after enumeration.
        if placement.after != HWND_BOTTOM
            && placement.after != HWND_TOP
            && (IsWindow(placement.after) == 0 || is_topmost(placement.after))
        {
            placement.after = HWND_BOTTOM;
        }
        if SetWindowPos(hwnd, placement.after, 0, 0, 0, 0, flags) == 0 {
            SetWindowPos(hwnd, HWND_BOTTOM, 0, 0, 0, 0, flags);
        }
        if is_topmost(hwnd) {
            SetWindowPos(hwnd, HWND_BOTTOM, 0, 0, 0, 0, flags);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;
    use windows_sys::{Win32::System::LibraryLoader::GetModuleHandleW, core::w};

    struct Window(HWND);

    impl Window {
        fn new(class: *const u16) -> Self {
            let hwnd = unsafe {
                CreateWindowExW(
                    WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                    class,
                    w!("柳溪村窗口层级回归测试"),
                    WS_POPUP,
                    -32000,
                    -32000,
                    80,
                    60,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    GetModuleHandleW(ptr::null()),
                    ptr::null(),
                )
            };
            assert!(!hwnd.is_null());
            unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
            Self(hwnd)
        }

        fn position(&self, after: HWND) {
            assert_ne!(
                unsafe {
                    SetWindowPos(
                        self.0,
                        after,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
                    )
                },
                0
            );
        }
    }

    impl Drop for Window {
        fn drop(&mut self) {
            unsafe { DestroyWindow(self.0) };
        }
    }

    fn above(upper: HWND, lower: HWND) -> bool {
        let mut next = unsafe { GetWindow(lower, GW_HWNDPREV) };
        for _ in 0..4096 {
            if next.is_null() {
                return false;
            }
            if next == upper {
                return true;
            }
            next = unsafe { GetWindow(next, GW_HWNDPREV) };
        }
        panic!("Window order did not terminate");
    }

    #[test]
    fn desktop_click_and_tray_reveal_keep_both_apps_above_village() {
        unsafe {
            let class = WNDCLASSW {
                lpfnWndProc: Some(DefWindowProcW),
                hInstance: GetModuleHandleW(ptr::null()),
                lpszClassName: w!("PecoFenceLiuxiLayerTest"),
                ..mem::zeroed()
            };
            assert_ne!(RegisterClassW(&class), 0);
        }
        let desktop = Window::new(w!("PecoFenceLiuxiLayerTest"));
        let app_a = Window::new(w!("STATIC"));
        let app_b = Window::new(w!("STATIC"));
        let village = Window::new(w!("STATIC"));
        desktop.position(HWND_BOTTOM);
        app_a.position(HWND_TOP);
        app_b.position(HWND_TOP);
        village.position(HWND_TOPMOST);
        let focus = unsafe { GetForegroundWindow() };
        place(village.0, false);
        assert!(!is_topmost(village.0));
        assert!(above(app_a.0, village.0));
        assert!(above(app_b.0, village.0));
        assert_eq!(unsafe { GetForegroundWindow() }, focus);

        // Desktop organizers can raise a fence after a desktop click.
        desktop.position(HWND_TOP);
        place(village.0, false);
        assert!(above(app_a.0, village.0));
        assert!(above(app_b.0, village.0));
        assert!(!is_topmost(village.0));

        unsafe { ShowWindow(village.0, SW_HIDE) };
        app_b.position(HWND_TOPMOST);
        place(village.0, true);
        assert_ne!(unsafe { IsWindowVisible(village.0) }, 0);
        assert!(above(app_a.0, village.0));
        assert!(above(app_b.0, village.0));
        assert!(!is_topmost(village.0));
        assert_eq!(unsafe { GetForegroundWindow() }, focus);
    }
}
