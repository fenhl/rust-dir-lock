// This code was copied and modified from https://github.com/heim-rs/heim
// Copyright (c) 2019 svartalf <https://svartalf.info>
//
// Permission is hereby granted, free of charge, to any
// person obtaining a copy of this software and associated
// documentation files (the "Software"), to deal in the
// Software without restriction, including without
// limitation the rights to use, copy, modify, merge,
// publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software
// is furnished to do so, subject to the following
// conditions:
//
// The above copyright notice and this permission notice
// shall be included in all copies or substantial portions
// of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
// ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
// TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
// PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
// SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
// OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
// IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

#![allow(unused_qualifications)]

use {
    std::io,
    crate::Error,
};
#[cfg(windows)] use {
    std::mem,
    winapi::{
        shared::{
            minwindef::DWORD,
            winerror,
        },
        um::{
            processthreadsapi,
            psapi,
            winnt,
        },
    },
    crate::IoResultExt as _,
};

#[cfg(unix)] type Pid = libc::pid_t;
#[cfg(windows)] type Pid = DWORD;

#[cfg(unix)]
pub fn pid_exists(pid: Pid) -> Result<bool, Error> {
    if pid == 0 { return Ok(true) }
    let result = unsafe { libc::kill(pid, 0) };
    Ok(if result == 0 {
        true
    } else {
        let e = io::Error::last_os_error();
        match e.raw_os_error() {
            Some(libc::ESRCH) => false,
            Some(libc::EPERM) => true,
            _ => true,
        }
    })
}

#[cfg(windows)]
pub fn pid_exists(pid: Pid) -> Result<bool, Error> {
    // Special case for "System Idle Process"
    if pid == 0 {
        return Ok(true);
    }

    const ACCESS: DWORD = winnt::PROCESS_QUERY_LIMITED_INFORMATION | winnt::PROCESS_VM_READ;

    let handle = unsafe { processthreadsapi::OpenProcess(ACCESS, 0, pid) };

    if handle.is_null() {
        match io::Error::last_os_error() {
            // Process exists, but we do not have an access to it
            err if err.kind() == io::ErrorKind::PermissionDenied => return Ok(true),

            // Notable error which might be returned here is
            // `ERROR_INVALID_PARAMETER` ("The parameter is incorrect").
            // Might mean that we are querying process with pid 0 (System Process)
            err if pid == 0
            && err.raw_os_error() == Some(winerror::ERROR_INVALID_PARAMETER as i32) => {
                return Ok(true)
            }
            // For other processes it is assumed that process is gone
            err if err.raw_os_error() == Some(winerror::ERROR_INVALID_PARAMETER as i32) => {
                return Ok(false)
            }

            e => return Err(e.at_unknown()),
        }
    }

    let mut code: DWORD = 0;

    let result = unsafe {
        // https://docs.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getexitcodeprocess
        processthreadsapi::GetExitCodeProcess(handle, &mut code)
    };

    if result == 0 {
        return Err(io::Error::last_os_error().at_unknown())
    }

    // TODO: Same as `psutil` this line is prone to error,
    // if the process had exited with code equal to `STILL_ACTIVE`
    if code == winapi::um::minwinbase::STILL_ACTIVE {
        Ok(true)
    } else {
        fn pids() -> Result<Vec<DWORD>, Error> {
            let mut processes = Vec::with_capacity(1024);
            let mut bytes_returned: DWORD = 0;

            loop {
                let cb = (processes.capacity() * mem::size_of::<DWORD>()) as DWORD;
                let result =
                    unsafe { psapi::EnumProcesses(processes.as_mut_ptr(), cb, &mut bytes_returned) };

                if result == 0 {
                    return Err(io::Error::last_os_error().at_unknown());
                }

                if cb == bytes_returned {
                    processes.reserve(1024);
                    continue;
                } else {
                    unsafe {
                        processes.set_len(bytes_returned as usize / mem::size_of::<DWORD>());
                    }
                    break;
                }
            }

            Ok(processes)
        }

        // Falling back to checking if pid is in list of running pids
        Ok(pids()?.contains(&pid))
    }
}
