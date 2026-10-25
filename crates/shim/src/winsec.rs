//! Windows: who may open the assistant pipe, and who serves it.
//!
//! The app creates every instance of the named pipe with a security descriptor that grants access to the current
//! user's SID only (owner and DACL, protected so nothing is inherited). The shim, before forwarding anything, asks the
//! pipe for the process that serves it and checks that this process runs as the same user; a pipe created first by a
//! program of another user is refused with a clear error.

use std::ffi::c_void;
use std::io;
use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};
use windows_sys::Win32::Security::Authorization::{ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
use windows_sys::Win32::Security::{GetLengthSid, GetTokenInformation, TokenUser, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER};
use windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION};

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CloseHandle(self.0) };
        }
    }
}

/// The user SID of a process, as raw bytes and as the string form (S-1-5-21-…).
fn process_user(process: HANDLE) -> io::Result<(Vec<u8>, String)> {
    let mut token: HANDLE = std::ptr::null_mut();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = Handle(token);
    let mut needed = 0u32;
    unsafe { GetTokenInformation(token.0, TokenUser, std::ptr::null_mut(), 0, &mut needed) };
    if needed == 0 {
        return Err(io::Error::last_os_error());
    }
    // u64 storage keeps TOKEN_USER aligned.
    let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
    if unsafe { GetTokenInformation(token.0, TokenUser, buf.as_mut_ptr().cast::<c_void>(), needed, &mut needed) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let user = unsafe { &*(buf.as_ptr().cast::<TOKEN_USER>()) };
    let sid = user.User.Sid;
    let len = unsafe { GetLengthSid(sid) } as usize;
    let bytes = unsafe { std::slice::from_raw_parts(sid.cast::<u8>(), len) }.to_vec();
    let mut wide: *mut u16 = std::ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut wide) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut n = 0;
    while unsafe { *wide.add(n) } != 0 {
        n += 1;
    }
    let text = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(wide, n) });
    unsafe { LocalFree(wide.cast::<c_void>()) };
    Ok((bytes, text))
}

/// The SID of the user running this process (raw bytes).
pub fn current_user_sid() -> io::Result<Vec<u8>> {
    process_user(unsafe { GetCurrentProcess() }).map(|(b, _)| b)
}

/// The SDDL of the pipe: owner, group and a protected DACL granting full access to `sid` only.
pub fn owner_only_sddl(sid: &str) -> String {
    format!("O:{sid}G:{sid}D:P(A;;GA;;;{sid})")
}

/// Security attributes for `CreateNamedPipeW` (tokio `ServerOptions::create_with_security_attributes_raw`): access
/// for the current user only. Keep it alive while creating pipe instances.
pub struct OwnerOnly {
    sd: PSECURITY_DESCRIPTOR,
    attrs: SECURITY_ATTRIBUTES,
}

// The descriptor is read-only after creation.
unsafe impl Send for OwnerOnly {}
unsafe impl Sync for OwnerOnly {}

impl OwnerOnly {
    pub fn new() -> io::Result<OwnerOnly> {
        let (_, sid) = process_user(unsafe { GetCurrentProcess() })?;
        let sddl: Vec<u16> = owner_only_sddl(&sid).encode_utf16().chain(std::iter::once(0)).collect();
        let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        if unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut sd, std::ptr::null_mut()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let attrs = SECURITY_ATTRIBUTES { nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: sd, bInheritHandle: 0 };
        Ok(OwnerOnly { sd, attrs })
    }

    /// The `SECURITY_ATTRIBUTES` pointer for the pipe creation call.
    pub fn as_ptr(&mut self) -> *mut c_void {
        (&mut self.attrs as *mut SECURITY_ATTRIBUTES).cast::<c_void>()
    }
}

impl Drop for OwnerOnly {
    fn drop(&mut self) {
        if !self.sd.is_null() {
            unsafe { LocalFree(self.sd) };
        }
    }
}

/// The user SID (raw bytes) of the process serving the pipe whose client end is `pipe`; None when it cannot be read
/// (for example a process of another user that this user may not inspect).
pub fn pipe_server_user(pipe: &impl std::os::windows::io::AsRawHandle) -> Option<Vec<u8>> {
    let mut pid = 0u32;
    if unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle() as HANDLE, &mut pid) } == 0 {
        return None;
    }
    let process = Handle(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) });
    if process.0.is_null() {
        return None;
    }
    process_user(process.0).ok().map(|(b, _)| b)
}
