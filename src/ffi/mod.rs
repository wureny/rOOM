mod bindings;
mod safe_wrapper;
mod types;

pub use safe_wrapper::SafeProcessHandle;
pub use types::{ProcessId, MemInfo, FfiError};

/// 提供一个安全的接口来访问底层系统调用
pub struct SystemInterface {
    // 内部字段
}

impl SystemInterface {
    /// 创建新的系统接口实例
    pub fn new() -> Self {
        Self { }
    }

    /// 安全地获取系统内存信息
    pub fn get_system_memory_info(&self) -> Result<MemInfo, FfiError> {
        // 实现安全的系统调用
        todo!()
    }

    /// 安全地终止进程
    pub fn kill_process(&self, pid: ProcessId) -> Result<(), FfiError> {
        // 实现安全的进程终止
        todo!()
    }
} 