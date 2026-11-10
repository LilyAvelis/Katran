//! Windows background processes with *no* inherited handles.
//!
//! Explicit null stdio on Command is not the ownership contract: this boundary
//! passes bInheritHandles=FALSE, so a long-lived viewer cannot retain API pipes.
use std::{ffi::OsStr, io, path::Path};

#[cfg(windows)]
pub struct BackgroundProcess {
    process: std::os::windows::io::OwnedHandle,
    pid: u32,
}
#[cfg(not(windows))]
pub struct BackgroundProcess;

impl BackgroundProcess {
    pub fn id(&self) -> u32 {
        #[cfg(windows)]
        {
            self.pid
        }
        #[cfg(not(windows))]
        {
            0
        }
    }
    pub fn try_wait(&mut self) -> io::Result<Option<u32>> {
        #[cfg(windows)]
        {
            native::try_wait(&self.process)
        }
        #[cfg(not(windows))]
        {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Windows required",
            ))
        }
    }
}
/// Spawn one absolute executable directly, with separately quoted OS arguments.
/// Dropping the returned process closes our handle and never terminates it.
pub fn spawn(executable: &Path, args: &[&OsStr]) -> io::Result<BackgroundProcess> {
    if !executable.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Background executable must be absolute",
        ));
    }
    #[cfg(windows)]
    {
        native::spawn(executable, args)
    }
    #[cfg(not(windows))]
    {
        let _ = args;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Windows required",
        ))
    }
}
#[cfg(windows)]
mod native {
    use super::*;
    use std::{
        ffi::c_void,
        mem::size_of,
        os::windows::{
            ffi::OsStrExt,
            io::{AsRawHandle, FromRawHandle, OwnedHandle},
        },
        ptr,
    };
    type Handle = *mut c_void;
    #[repr(C)]
    struct StartupInfo {
        size: u32,
        reserved: *mut u16,
        desktop: *mut u16,
        title: *mut u16,
        x: u32,
        y: u32,
        x_size: u32,
        y_size: u32,
        x_count: u32,
        y_count: u32,
        fill: u32,
        flags: u32,
        show: u16,
        reserved_size: u16,
        reserved_bytes: *mut u8,
        stdin: Handle,
        stdout: Handle,
        stderr: Handle,
    }
    #[repr(C)]
    struct ProcessInfo {
        process: Handle,
        thread: Handle,
        pid: u32,
        thread_id: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateProcessW(
            application: *const u16,
            command: *mut u16,
            process_security: *const c_void,
            thread_security: *const c_void,
            inherit: i32,
            flags: u32,
            environment: *const c_void,
            directory: *const u16,
            startup: *const StartupInfo,
            info: *mut ProcessInfo,
        ) -> i32;
        fn WaitForSingleObject(handle: Handle, timeout: u32) -> u32;
        fn GetExitCodeProcess(handle: Handle, code: *mut u32) -> i32;
    }
    fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
        let mut value: Vec<u16> = value.encode_wide().collect();
        if value.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Embedded NUL in process argument",
            ));
        }
        value.push(0);
        Ok(value)
    }
    // Windows CRT/.NET argv quoting: backslashes before quotes and the final
    // quote are doubled. Ordinary Unicode, spaces and non-shell punctuation
    // remain literal. No shell ever sees this command line.
    fn append_argument(output: &mut Vec<u16>, argument: &OsStr) -> io::Result<()> {
        let encoded = wide(argument)?;
        output.push(b'"' as u16);
        let mut slashes = 0;
        for &ch in &encoded[..encoded.len() - 1] {
            if ch == b'\\' as u16 {
                slashes += 1;
                continue;
            }
            if ch == b'"' as u16 {
                output.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2 + 1));
            } else {
                output.extend(std::iter::repeat_n(b'\\' as u16, slashes));
            }
            slashes = 0;
            output.push(ch);
        }
        output.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
        output.push(b'"' as u16);
        Ok(())
    }
    pub(super) fn spawn(executable: &Path, args: &[&OsStr]) -> io::Result<BackgroundProcess> {
        let application = wide(executable.as_os_str())?;
        let mut command = Vec::new();
        append_argument(&mut command, executable.as_os_str())?;
        for argument in args {
            command.push(b' ' as u16);
            append_argument(&mut command, argument)?;
        }
        command.push(0);
        if command.len() > 32767 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Windows command line exceeds limit",
            ));
        }
        let directory = wide(
            executable
                .parent()
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Executable has no parent directory",
                    )
                })?
                .as_os_str(),
        )?;
        // No STARTF_USESTDHANDLES and no inheritable security attributes. The
        // explicit FALSE below prevents inheritance of *any* parent handle.
        let mut startup: StartupInfo = unsafe { std::mem::zeroed() };
        startup.size = size_of::<StartupInfo>() as u32;
        let mut info: ProcessInfo = unsafe { std::mem::zeroed() };
        let ok = unsafe {
            CreateProcessW(
                application.as_ptr(),
                command.as_mut_ptr(),
                ptr::null(),
                ptr::null(),
                0,
                0x08000000,
                ptr::null(),
                directory.as_ptr(),
                &startup,
                &mut info,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        // Both handles are owned by this successful call. Close the thread
        // handle immediately; retain only the process handle for readiness.
        let process = unsafe { OwnedHandle::from_raw_handle(info.process) };
        drop(unsafe { OwnedHandle::from_raw_handle(info.thread) });
        Ok(BackgroundProcess {
            process,
            pid: info.pid,
        })
    }
    pub(super) fn try_wait(process: &OwnedHandle) -> io::Result<Option<u32>> {
        match unsafe { WaitForSingleObject(process.as_raw_handle(), 0) } {
            0 => {
                let mut code = 0;
                if unsafe { GetExitCodeProcess(process.as_raw_handle(), &mut code) } == 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(Some(code))
            }
            258 => Ok(None),
            _ => Err(io::Error::last_os_error()),
        }
    }
}
