use std::process::Child;

/// 将子进程绑定到主进程的 Windows Job Object（`KILL_ON_JOB_CLOSE`），
/// 确保即使开发模式按 Ctrl+C 或异常退出，也不会遗留 `llama-server` / Python / PowerShell 僵尸进程占满显存或端口。
pub fn attach_child_to_job(child: &Child) {
    #[cfg(target_os = "windows")]
    win::attach(child);
    #[cfg(not(target_os = "windows"))]
    let _ = child;
}

/// 若目标本地端口被上一次异常退出遗留的同类进程占用，先行清理后再拉起新实例，避免端口冲突或重复占满显存。
pub fn free_stale_port(port: u16, expected_exe_substr: &str) {
    #[cfg(target_os = "windows")]
    win::free_port(port, expected_exe_substr);
    #[cfg(not(target_os = "windows"))]
    let _ = (port, expected_exe_substr);
}

#[cfg(target_os = "windows")]
mod win {
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};
    use std::os::windows::io::AsRawHandle;
    use std::os::windows::process::CommandExt;
    use std::process::{Child, Command};
    use std::sync::OnceLock;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS: i32 = 9;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;

    #[repr(C)]
    #[derive(Copy, Clone)]
    struct JobObjectBasicLimitInformation {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    struct IoCounters {
        read_operation_count: u64,
        write_operation_count: u64,
        other_operation_count: u64,
        read_transfer_count: u64,
        write_transfer_count: u64,
        other_transfer_count: u64,
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    struct JobObjectExtendedLimitInformation {
        basic_limit_information: JobObjectBasicLimitInformation,
        io_info: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    struct JobHandle(*mut c_void);
    unsafe impl Send for JobHandle {}
    unsafe impl Sync for JobHandle {}

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateJobObjectW(lp_job_attributes: *mut c_void, lp_name: *const u16) -> *mut c_void;
        fn SetInformationJobObject(
            h_job: *mut c_void,
            job_object_information_class: i32,
            lp_job_object_information: *const c_void,
            cb_job_object_information_length: u32,
        ) -> i32;
        fn AssignProcessToJobObject(h_job: *mut c_void, h_process: *mut c_void) -> i32;
    }

    static JOB: OnceLock<Option<JobHandle>> = OnceLock::new();

    fn global_job() -> Option<*mut c_void> {
        JOB.get_or_init(|| unsafe {
            let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
            if job.is_null() {
                return None;
            }
            let mut info: JobObjectExtendedLimitInformation = zeroed();
            info.basic_limit_information.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                job,
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS,
                &info as *const _ as *const c_void,
                size_of::<JobObjectExtendedLimitInformation>() as u32,
            );
            if ok == 0 {
                None
            } else {
                Some(JobHandle(job))
            }
        })
        .as_ref()
        .map(|h| h.0)
    }

    pub fn attach(child: &Child) {
        if let Some(job) = global_job() {
            let handle = child.as_raw_handle();
            unsafe {
                let _ = AssignProcessToJobObject(job, handle);
            }
        }
    }

    pub fn free_port(port: u16, expected_exe_substr: &str) {
        let mut cmd = Command::new("netstat");
        cmd.args(["-ano", "-p", "tcp"]);
        cmd.creation_flags(CREATE_NO_WINDOW);
        let Ok(out) = cmd.output() else {
            return;
        };
        let text = String::from_utf8_lossy(&out.stdout);
        let port_pat = format!(":{port}");
        let self_pid = std::process::id();
        let needle = expected_exe_substr.to_ascii_lowercase();

        let mut pids: Vec<u32> = Vec::new();
        for line in text.lines() {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() < 5 {
                continue;
            }
            let local_addr = cols[1];
            let state = cols[3];
            if !state.eq_ignore_ascii_case("LISTENING") {
                continue;
            }
            if !local_addr.ends_with(&port_pat) {
                continue;
            }
            if let Ok(pid) = cols[4].parse::<u32>() {
                if pid > 4 && pid != self_pid && !pids.contains(&pid) {
                    pids.push(pid);
                }
            }
        }

        for pid in pids {
            let mut check = Command::new("tasklist");
            check.args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"]);
            check.creation_flags(CREATE_NO_WINDOW);
            if let Ok(tl) = check.output() {
                let img = String::from_utf8_lossy(&tl.stdout).to_ascii_lowercase();
                if img.contains(&needle) {
                    let mut kill = Command::new("taskkill");
                    kill.args(["/F", "/T", "/PID", &pid.to_string()]);
                    kill.creation_flags(CREATE_NO_WINDOW);
                    let _ = kill.output();
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::process::Stdio;

        #[test]
        fn job_object_initializes_and_attaches_child() {
            assert_eq!(size_of::<JobObjectExtendedLimitInformation>(), 144);
            let mut cmd = Command::new("cmd");
            cmd.args(["/C", "exit", "0"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(CREATE_NO_WINDOW);
            let mut child = cmd.spawn().expect("spawn cmd");
            attach(&child);
            assert!(global_job().is_some());
            let _ = child.wait();
        }
    }
}
