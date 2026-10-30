// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::{
    ffi::c_void,
    io,
    mem::size_of,
    os::windows::{
        ffi::OsStringExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::PathBuf,
};

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct FileTime {
    low: u32,
    high: u32,
}
#[repr(C)]
struct ProcessEntry {
    size: u32,
    usage: u32,
    pid: u32,
    heap: usize,
    module: u32,
    threads: u32,
    parent: u32,
    priority: i32,
    flags: u32,
    name: [u16; 260],
}
#[repr(C)]
struct ThreadEntry {
    size: u32,
    usage: u32,
    id: u32,
    owner: u32,
    priority: i32,
    delta: i32,
    flags: u32,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    #[link_name = "OpenProcess"]
    fn open_process_raw(access: u32, inherit: i32, pid: u32) -> *mut c_void;
    #[link_name = "OpenThread"]
    fn open_thread_raw(access: u32, inherit: i32, id: u32) -> *mut c_void;
    #[link_name = "WaitForSingleObject"]
    fn wait(handle: *mut c_void, millis: u32) -> u32;
    #[link_name = "GetProcessTimes"]
    fn times(
        handle: *mut c_void,
        creation: *mut FileTime,
        exit: *mut FileTime,
        kernel: *mut FileTime,
        user: *mut FileTime,
    ) -> i32;
    #[link_name = "QueryFullProcessImageNameW"]
    fn image_raw(handle: *mut c_void, flags: u32, path: *mut u16, size: *mut u32) -> i32;
    #[link_name = "CreateToolhelp32Snapshot"]
    fn snapshot(flags: u32, pid: u32) -> *mut c_void;
    #[link_name = "Process32FirstW"]
    fn process_first(handle: *mut c_void, entry: *mut ProcessEntry) -> i32;
    #[link_name = "Process32NextW"]
    fn process_next(handle: *mut c_void, entry: *mut ProcessEntry) -> i32;
    #[link_name = "Thread32First"]
    fn thread_first(handle: *mut c_void, entry: *mut ThreadEntry) -> i32;
    #[link_name = "Thread32Next"]
    fn thread_next(handle: *mut c_void, entry: *mut ThreadEntry) -> i32;
    #[link_name = "GetProcessIdOfThread"]
    fn thread_owner(handle: *mut c_void) -> u32;
    #[link_name = "SuspendThread"]
    fn suspend_raw(handle: *mut c_void) -> u32;
    #[link_name = "ResumeThread"]
    fn resume_raw(handle: *mut c_void) -> u32;
}

fn owned(raw: *mut c_void) -> io::Result<OwnedHandle> {
    if raw.is_null() || raw as isize == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
}
pub(super) fn process(pid: u32) -> io::Result<OwnedHandle> {
    owned(unsafe { open_process_raw(0x1000 | 0x00100000, 0, pid) })
}
pub(super) fn alive(handle: &OwnedHandle) -> io::Result<bool> {
    match unsafe { wait(handle.as_raw_handle(), 0) } {
        258 => Ok(true),
        0 => Ok(false),
        _ => Err(io::Error::last_os_error()),
    }
}
pub(super) fn created(handle: &OwnedHandle) -> io::Result<u64> {
    let (mut creation, mut exit, mut kernel, mut user) = (
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
    );
    if unsafe {
        times(
            handle.as_raw_handle(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok((u64::from(creation.high) << 32) | u64::from(creation.low))
}
pub(super) fn image(handle: &OwnedHandle) -> io::Result<PathBuf> {
    let mut buffer = vec![0u16; 32768];
    let mut length = buffer.len() as u32;
    if unsafe { image_raw(handle.as_raw_handle(), 0, buffer.as_mut_ptr(), &mut length) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(PathBuf::from(std::ffi::OsString::from_wide(
        &buffer[..length as usize],
    )))
}
pub(super) fn parent(pid: u32) -> io::Result<u32> {
    let handle = owned(unsafe { snapshot(2, 0) })?;
    let mut entry: ProcessEntry = unsafe { std::mem::zeroed() };
    entry.size = size_of::<ProcessEntry>() as u32;
    let mut present = unsafe { process_first(handle.as_raw_handle(), &mut entry) };
    let mut visited = 0;
    while present != 0 {
        if entry.size as usize != size_of::<ProcessEntry>() {
            return Err(io::Error::other("process snapshot layout refused"));
        }
        visited += 1;
        if visited > 8192 {
            return Err(io::Error::other("process snapshot exceeded its bound"));
        }
        if entry.pid == pid {
            return Ok(entry.parent);
        }
        entry.size = size_of::<ProcessEntry>() as u32;
        present = unsafe { process_next(handle.as_raw_handle(), &mut entry) };
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() != Some(18) {
        return Err(error);
    }
    Err(io::Error::other("owned process absent from snapshot"))
}
pub(super) fn threads(pid: u32) -> io::Result<Vec<u32>> {
    let handle = owned(unsafe { snapshot(4, 0) })?;
    let mut entry: ThreadEntry = unsafe { std::mem::zeroed() };
    entry.size = size_of::<ThreadEntry>() as u32;
    let mut present = unsafe { thread_first(handle.as_raw_handle(), &mut entry) };
    let mut found = Vec::new();
    let mut visited = 0;
    while present != 0 {
        if entry.size as usize != size_of::<ThreadEntry>() {
            return Err(io::Error::other("thread snapshot layout refused"));
        }
        visited += 1;
        if visited > 65536 {
            return Err(io::Error::other("thread snapshot exceeded its bound"));
        }
        if entry.owner == pid {
            found.push(entry.id);
            if found.len() > 128 {
                return Err(io::Error::other("owned encoder thread bound exceeded"));
            }
        }
        entry.size = size_of::<ThreadEntry>() as u32;
        present = unsafe { thread_next(handle.as_raw_handle(), &mut entry) };
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() != Some(18) {
        return Err(error);
    }
    found.sort_unstable();
    Ok(found)
}
pub(super) fn thread(id: u32, pid: u32) -> io::Result<OwnedHandle> {
    let handle = owned(unsafe { open_thread_raw(2 | 0x800 | 0x00100000, 0, id) })?;
    if unsafe { thread_owner(handle.as_raw_handle()) } != pid {
        return Err(io::Error::other("thread owner changed"));
    }
    Ok(handle)
}
pub(super) fn suspend(handle: &OwnedHandle) -> io::Result<u32> {
    let count = unsafe { suspend_raw(handle.as_raw_handle()) };
    if count == u32::MAX {
        return Err(io::Error::last_os_error());
    }
    Ok(count)
}
pub(super) fn resume(handle: &OwnedHandle) -> io::Result<()> {
    if unsafe { resume_raw(handle.as_raw_handle()) } == u32::MAX {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
