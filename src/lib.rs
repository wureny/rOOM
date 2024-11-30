//! Room - A Rust implementation of the Linux OOM Killer
//! 
//! This library provides a memory pressure monitoring and process termination
//! system similar to the Linux OOM Killer, but implemented in Rust with
//! additional safety guarantees and improved configurability.

// 导出所有公共模块
pub mod ffi;
pub mod linux;
pub mod oom;

// 重新导出常用类型，使其可以直接从 crate 根访问
pub use crate::ffi::types::{ProcessId, Result, SystemError};
pub use crate::oom::killer::OOMKiller;
pub use crate::oom::pressure::PressureDetector;
pub use crate::oom::score::OOMScorer;
pub use crate::oom::selector::ProcessSelector;

/// 库的版本信息
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 初始化日志系统
/// 
/// 这个函数应该在使用库之前调用
pub fn init() -> Result<()> {
    // 初始化日志
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    env_logger::init();

    // 检查运行时环境
    check_environment()?;

    Ok(())
}

/// 检查运行时环境
fn check_environment() -> Result<()> {
    // 检查是否有足够的权限访问 /proc
    if !std::path::Path::new("/proc").exists() {
        return Err(SystemError::PermissionDenied);
    }

    // 检查是否能读取系统内存信息
    crate::linux::proc::get_memory_info()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        assert!(init().is_ok());
    }

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }
} 