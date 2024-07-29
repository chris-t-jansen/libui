//! Functionality related to creating, managing, and destroying GUI windows.

use callback_helpers::{from_void_ptr, to_heap_ptr};
use controls::Control;
use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::mem;
use std::os::raw::{c_int, c_void};
use std::path::PathBuf;
use ui::UI;
use libui_ffi::{self, uiControl, uiFreeText, uiWindow};

thread_local! {
    static WINDOWS: RefCell<Vec<Window>> = RefCell::new(Vec::new())
}

/// A `Window` can either have a menubar or not; this enum represents that decision.
#[derive(Clone, Copy, Debug)]
pub enum WindowType {
    HasMenubar,
    NoMenubar,
}

define_control! {
    /// Contains a single child control and displays it and its children in a window on the screen.
    rust_type: Window,
    sys_type: uiWindow
}

impl Window {
    /// Create a new window with the given title, width, height, and type.
    /// By default, when a new window is created, it will cause the application to quit when closed.
    /// The user can prevent this by adding a custom `on_closing` behavior.
    pub fn new(_ctx: &UI, title: &str, width: c_int, height: c_int, t: WindowType) -> Window {
        let has_menubar = match t {
            WindowType::HasMenubar => true,
            WindowType::NoMenubar => false,
        };
        let mut window = unsafe {
            let c_string = CString::new(title.as_bytes().to_vec()).unwrap();
            let window = Window::from_raw(libui_ffi::uiNewWindow(
                c_string.as_ptr(),
                width,
                height,
                has_menubar as c_int,
            ));

            WINDOWS.with(|windows| windows.borrow_mut().push(window.clone()));

            window
        };

        // Windows, by default, quit the application on closing.
        let ui = _ctx.clone();
        window.on_closing(_ctx, move |_| {
            ui.quit();
        });

        // Windows, by default, draw margins
        window.set_margined(true);

        window
    }

    /// Get the current title of the window.
    pub fn title(&self) -> String {
        unsafe {
            CStr::from_ptr(libui_ffi::uiWindowTitle(self.uiWindow))
                .to_string_lossy()
                .into_owned()
        }
    }

    /// Get a reference to the current title of the window.
    pub fn title_ref(&self) -> &CStr {
        unsafe { &CStr::from_ptr(libui_ffi::uiWindowTitle(self.uiWindow)) }
    }

    /// Set the window's title to the given string.
    pub fn set_title(&mut self, title: &str) {
        unsafe {
            let c_string = CString::new(title.as_bytes().to_vec()).unwrap();
            libui_ffi::uiWindowSetTitle(self.uiWindow, c_string.as_ptr())
        }
    }

    /// Set a callback to be run when the window closes.
    ///
    /// This is often used on the main window of an application to quit
    /// the application when the window is closed.
    pub fn on_closing<'ctx, F>(&mut self, _ctx: &'ctx UI, callback: F)
    where
        F: FnMut(&mut Window) + 'static,
    {
        extern "C" fn c_callback<G>(window: *mut uiWindow, data: *mut c_void) -> i32
        where
            G: FnMut(&mut Window),
        {
            let mut window = Window { uiWindow: window };
            unsafe {
                from_void_ptr::<G>(data)(&mut window);
            }
            0
        }

        unsafe {
            libui_ffi::uiWindowOnClosing(self.uiWindow, Some(c_callback::<F>), to_heap_ptr(callback));
        }
    }

    /// Gets the window position on the screen.
    /// Coordinates are measured from the top-left corner of the screen.
    /// 
    /// This method may return inaccurate or dummy values on Unix platforms.
    pub fn position(&self) -> (i32, i32) {
        let mut x_pos: c_int = 0;
        let mut y_pos: c_int = 0;
        unsafe { libui_ffi::uiWindowPosition(self.uiWindow, &mut x_pos, &mut y_pos) }

        (x_pos.into(), y_pos.into())
    }

    /// Moves the window to the specified position on the screen.
    /// Coordinates are measured from the top-left corner of the screen.
    /// 
    /// This method is merely a hint and may be ignored on Unix platforms.
    pub fn set_position(&mut self, x_position: i32, y_position: i32) {
        unsafe { libui_ffi::uiWindowSetPosition(self.uiWindow, x_position, y_position) }
    }

    /// Sets a callback to be run when the user changes the window's position.
    ///
    /// Note that this callback does not trigger when the window is moved through the `set_position` method.
    /// It triggers when the user drags the window across the screen, not when the application changes its own position.
    pub fn on_position_changed<'ctx, F>(&mut self, callback: F)
    where
        F: FnMut(&mut Window) + 'static,
    {
        extern "C" fn c_callback<G>(window: *mut uiWindow, data: *mut c_void)
        where
            G: FnMut(&mut Window),
        {
            let mut window = Window { uiWindow: window };
            unsafe {
                from_void_ptr::<G>(data)(&mut window);
            }
        }

        unsafe {
            libui_ffi::uiWindowOnPositionChanged(self.uiWindow, Some(c_callback::<F>), to_heap_ptr(callback));
        }
    }

    /// Check whether or not this window has margins around the edges.
    pub fn margined(&self) -> bool {
        unsafe { libui_ffi::uiWindowMargined(self.uiWindow) != 0 }
    }

    /// Set whether or not the window has margins around the edges.
    pub fn set_margined(&mut self, margined: bool) {
        unsafe { libui_ffi::uiWindowSetMargined(self.uiWindow, margined as c_int) }
    }

    /// Check whether or not this window is resizeable by the user at runtime.
    pub fn resizeable(&self) -> bool {
        unsafe { libui_ffi::uiWindowResizeable(self.uiWindow) != 0 }
    }

    /// Set whether or not this window is resizeable by the user at runtime.
    /// 
    /// This method is merely a hint and may be ignored by the system.
    pub fn set_resizeable(&mut self, resizeable: bool) {
        unsafe { libui_ffi::uiWindowSetResizeable(self.uiWindow, resizeable as c_int) }
    }

    /// Sets the window's child widget. The window can only have one child widget at a time.
    pub fn set_child<T: Into<Control>>(&mut self, child: T) {
        unsafe { libui_ffi::uiWindowSetChild(self.uiWindow, child.into().as_ui_control()) }
    }

    /// Allow the user to select an existing file using the systems file dialog
    pub fn open_file(&self) -> Option<PathBuf> {
        let ptr = unsafe { libui_ffi::uiOpenFile(self.uiWindow) };
        if ptr.is_null() {
            return None;
        };
        let path_string: String = unsafe { CStr::from_ptr(ptr).to_string_lossy().into() };
        unsafe {
            uiFreeText(ptr);
        }
        Some(path_string.into())
    }

    /// Allow the user to select a new or existing file using the systems file dialog.
    pub fn save_file(&self) -> Option<PathBuf> {
        let ptr = unsafe { libui_ffi::uiSaveFile(self.uiWindow) };
        if ptr.is_null() {
            return None;
        };
        let path_string: String = unsafe { CStr::from_ptr(ptr).to_string_lossy().into() };
        unsafe {
            uiFreeText(ptr);
        }
        Some(path_string.into())
    }

    /// Allow the user to select a single folder using the systems folder dialog.
    pub fn open_folder(&self) -> Option<PathBuf> {
        let ptr = unsafe { libui_ffi::uiOpenFolder(self.uiWindow) };
        if ptr.is_null() {
            return None;
        };
        let path_string: String = unsafe { CStr::from_ptr(ptr).to_string_lossy().into() };
        unsafe {
            uiFreeText(ptr);
        }
        Some(path_string.into())
    }

    /// Open a generic message box to show a message to the user.
    /// Returns when the user acknowledges the message.
    pub fn modal_msg(&self, title: &str, description: &str) {
        unsafe {
            let c_title = CString::new(title.as_bytes().to_vec()).unwrap();
            let c_description = CString::new(description.as_bytes().to_vec()).unwrap();
            libui_ffi::uiMsgBox(self.uiWindow, c_title.as_ptr(), c_description.as_ptr())
        }
    }

    /// Open an error-themed message box to show a message to the user.
    /// Returns when the user acknowledges the message.
    pub fn modal_err(&self, title: &str, description: &str) {
        unsafe {
            let c_title = CString::new(title.as_bytes().to_vec()).unwrap();
            let c_description = CString::new(description.as_bytes().to_vec()).unwrap();
            libui_ffi::uiMsgBoxError(self.uiWindow, c_title.as_ptr(), c_description.as_ptr())
        }
    }

    pub unsafe fn destroy_all_windows() {
        WINDOWS.with(|windows| {
            let mut windows = windows.borrow_mut();
            for window in windows.drain(..) {
                window.destroy();
            }
        })
    }

    /// Destroys a Window. Any use of the control after this is use-after-free; therefore, this
    /// is marked unsafe.
    pub unsafe fn destroy(&self) {
        // Don't check for initialization here since this can be run during deinitialization.
        libui_ffi::uiControlDestroy(self.uiWindow as *mut libui_ffi::uiControl)
    }
}
