//! Helpers for checking whether Windows Explorer is running and ending it.

use std::mem;

use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};

/// Return the process IDs of all running `explorer.exe` processes.
fn process_ids() -> Vec<u32> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Vec::new();
        }

        let mut entry: PROCESSENTRY32W = mem::zeroed();
        entry.dwSize = mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ids = Vec::new();

        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                let name = String::from_utf16_lossy(&entry.szExeFile);
                if name.trim_end_matches('\0').eq_ignore_ascii_case("explorer.exe") {
                    ids.push(entry.th32ProcessID);
                }
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }

        CloseHandle(snapshot);
        ids
    }
}

/// Check whether Explorer is currently running.
pub fn is_running() -> bool {
    !process_ids().is_empty()
}

/// Terminate all currently running Explorer processes.
pub fn kill() -> bool {
    let ids = process_ids();
    let mut terminated_any = false;

    for process_id in ids {
        unsafe {
            let process = OpenProcess(PROCESS_TERMINATE, 0, process_id);
            if !process.is_null() {
                if TerminateProcess(process, 1) != 0 {
                    terminated_any = true;
                }
                CloseHandle(process);
            }
        }
    }

    terminated_any
}
