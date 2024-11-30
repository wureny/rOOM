use std::os::raw::{c_int, c_ulong};
use std::fmt;

/// 进程ID的安全包装
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ProcessId(pub(crate) c_int);

impl ProcessId {
    /// 创建新的ProcessId，确保值有效
    pub fn new(pid: i32) -> Option<Self> {
        if pid > 0 {
            Some(ProcessId(pid))
        } else {
            None
        }
    }

    pub fn as_raw(&self) -> c_int {
        self.0
    }
}

/// 系统内存信息的安全包装
#[derive(Debug, Clone)]
pub struct SystemInfo {
    pub uptime: u64,
    pub total_ram: u64,
    pub free_ram: u64,
    pub shared_ram: u64,
    pub buffer_ram: u64,
    pub total_swap: u64,
    pub free_swap: u64,
    pub procs: u16,
}

/// 错误类型
#[derive(Debug, thiserror::Error)]
pub enum SystemError {
    #[error("Invalid process ID: {0}")]
    InvalidPid(i32),
    #[error("System call failed: {0}")]
    SyscallError(#[from] std::io::Error),
    #[error("Permission denied")]
    PermissionDenied,
    #[error("Process not found")]
    ProcessNotFound,
}

pub type Result<T> = std::result::Result<T, SystemError>; 