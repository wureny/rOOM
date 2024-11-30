use super::bindings;
use super::types::{ProcessId, SystemInfo, SystemError, Result};
use std::mem::MaybeUninit;
use std::os::raw::c_int;
use std::io;

pub struct SystemInterface;

impl SystemInterface {
    /// 创建新的系统接口实例
    pub fn new() -> Self {
        Self
    }

    /// 安全地获取系统信息
    /// 
    /// # 返回值
    /// 
    /// 返回包含系统信息的 `SystemInfo` 结构体
    /// 
    /// # 错误
    /// 
    /// 如果系统调用失败，返回 `SystemError::SyscallError`
    pub fn get_system_info(&self) -> Result<SystemInfo> {
        // 使用 MaybeUninit 避免未初始化内存
        let mut info = MaybeUninit::uninit();
        
        // 进行系统调用
        let result = unsafe {
            bindings::sysinfo(info.as_mut_ptr())
        };

        if result == 0 {
            // 安全：sysinfo成功时会完全初始化结构体
            let info = unsafe { info.assume_init() };
            
            Ok(SystemInfo {
                uptime: info.uptime as u64,
                total_ram: info.totalram as u64,
                free_ram: info.freeram as u64,
                shared_ram: info.sharedram as u64,
                buffer_ram: info.bufferram as u64,
                total_swap: info.totalswap as u64,
                free_swap: info.freeswap as u64,
                procs: info.procs,
            })
        } else {
            Err(SystemError::SyscallError(io::Error::last_os_error()))
        }
    }

    /// 安全地发送信号给进程
    /// 
    /// # 参数
    /// 
    /// * `pid` - 目标进程ID
    /// * `signal` - 要发送的信号
    /// 
    /// # 错误
    /// 
    /// * `SystemError::InvalidPid` - 如果PID无效
    /// * `SystemError::ProcessNotFound` - 如果进程不存在
    /// * `SystemError::PermissionDenied` - 如果没有权限
    pub fn kill(&self, pid: ProcessId, signal: c_int) -> Result<()> {
        let result = unsafe {
            bindings::kill(pid.as_raw(), signal)
        };

        match result {
            0 => Ok(()),
            _ => {
                let err = io::Error::last_os_error();
                match err.kind() {
                    io::ErrorKind::PermissionDenied => Err(SystemError::PermissionDenied),
                    io::ErrorKind::NotFound => Err(SystemError::ProcessNotFound),
                    _ => Err(SystemError::SyscallError(err)),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_info() {
        let sys = SystemInterface::new();
        let info = sys.get_system_info().expect("Failed to get system info");
        
        // 验证返回的信息是否合理
        assert!(info.total_ram > 0);
        assert!(info.total_ram >= info.free_ram);
        assert!(info.procs > 0);
    }

    #[test]
    fn test_invalid_pid() {
        let sys = SystemInterface::new();
        let pid = ProcessId::new(-1);
        assert!(pid.is_none());
    }
} 