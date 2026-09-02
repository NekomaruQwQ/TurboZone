use std::fmt;
use std::hash::{Hash, Hasher};

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::HMONITOR;

pub trait HandleTrait: Sized + Copy + Eq + Default + fmt::Debug {
    fn addr(self) -> usize;
}

impl HandleTrait for HWND {
    fn addr(self) -> usize { self.0.addr() }
}

impl HandleTrait for HMONITOR {
    fn addr(self) -> usize { self.0.addr() }
}

/// Newtype wrapping a Win32 handle that provides extra implementation for
/// common traits.
///
/// Note that Windows may destroy or reuse a handle after a snapshot, so
/// native actions remain fallible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Handle<T: HandleTrait>(pub T);

impl<T: HandleTrait> Handle<T> {
    pub fn invalid() -> Self { Self(T::default()) }

    /// Returns the native identity for UI keys and diagnostics, without dereferencing it.
    pub fn value(self) -> usize { self.0.addr() }
}

impl<T: HandleTrait> Hash for Handle<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value().hash(state);
    }
}

impl fmt::Display for Handle<HWND> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use crate::native;

        let title = native::get_window_text(self.0);
        if !title.is_empty() {
            write!(f, "0x{:X} (\"{title}\")", self.value())
        } else {
            write!(f, "0x{:X}", self.value())
        }
    }
}