use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use crate::ffi::types::{ProcessId, SystemError, Result};

/// 进程的内存统计信息
#[derive(Debug, Clone)]
pub struct ProcessMemInfo {
    pub vm_peak: u64,      // 进程使用的虚拟内存峰值
    pub vm_size: u64,      // 当前虚拟内存使用量
    pub vm_rss: u64,       // 物理内存使用量
    pub vm_swap: u64,      // swap使用量
    pub oom_score: i32,    // 系统计算的OOM分数
    pub oom_score_adj: i32, // OOM分数调整值
}

/// 进程的基本信息
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: ProcessId,
    pub name: String,
    pub state: String,
    pub ppid: i32,
    pub mem_info: ProcessMemInfo,
}

impl ProcessInfo {
    /// 从/proc文件系统读取指定进程的信息
    /// 
    /// # 参数
    /// 
    /// * `pid` - 进程ID
    /// 
    /// # 返回值
    /// 
    /// 返回包含进程信息的 ProcessInfo 结构体
    pub fn from_pid(pid: ProcessId) -> Result<Self> {
        let status_path = format!("/proc/{}/status", pid.as_raw());
        let oom_score_path = format!("/proc/{}/oom_score", pid.as_raw());
        let oom_adj_path = format!("/proc/{}/oom_score_adj", pid.as_raw());

        // 读取进程状态信息
        let mut name = String::new();
        let mut state = String::new();
        let mut ppid = 0;
        let mut vm_peak = 0;
        let mut vm_size = 0;
        let mut vm_rss = 0;
        let mut vm_swap = 0;

        let file = File::open(&status_path).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                SystemError::ProcessNotFound
            } else {
                SystemError::SyscallError(e)
            }
        })?;

        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = line?;
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() < 2 {
                continue;
            }

            let key = parts[0].trim();
            let value = parts[1].trim();

            match key {
                "Name" => name = value.to_string(),
                "State" => state = value.to_string(),
                "PPid" => ppid = value.parse().unwrap_or(0),
                "VmPeak" => vm_peak = parse_kb_value(value),
                "VmSize" => vm_size = parse_kb_value(value),
                "VmRSS" => vm_rss = parse_kb_value(value),
                "VmSwap" => vm_swap = parse_kb_value(value),
                _ => {}
            }
        }

        // 读取OOM分数
        let oom_score = read_proc_value(&oom_score_path)?;
        let oom_score_adj = read_proc_value(&oom_adj_path)?;

        Ok(ProcessInfo {
            pid,
            name,
            state,
            ppid,
            mem_info: ProcessMemInfo {
                vm_peak,
                vm_size,
                vm_rss,
                vm_swap,
                oom_score,
                oom_score_adj,
            },
        })
    }

    /// 判断进程是否可以被OOM killer终止
    pub fn is_oomable(&self) -> bool {
        // 系统进程通常不应该被OOM killer终止
        !self.name.starts_with('[') && 
        self.oom_score_adj > -1000 &&
        self.state != "Z" // 不终止僵尸进程
    }
}

/// 解析/proc中的KB值（例如："1024 kB"）
fn parse_kb_value(value: &str) -> u64 {
    value.split_whitespace()
        .next()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0)
}

/// 读取/proc中的单个数值
fn read_proc_value(path: &str) -> Result<i32> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            SystemError::ProcessNotFound
        } else {
            SystemError::SyscallError(e)
        }
    })?;
    
    content.trim().parse().map_err(|_| {
        SystemError::SyscallError(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid proc value"
        ))
    })
}

/// 获取系统中所有进程的列表
pub fn get_all_processes() -> Result<Vec<ProcessInfo>> {
    let proc_dir = Path::new("/proc");
    let mut processes = Vec::new();

    for entry in proc_dir.read_dir().map_err(SystemError::SyscallError)? {
        let entry = entry.map_err(SystemError::SyscallError)?;
        let file_name = entry.file_name();
        
        // 只处理数字名称的目录（即PID目录）
        if let Some(pid_str) = file_name.to_str() {
            if let Ok(pid_num) = pid_str.parse::<i32>() {
                if let Some(pid) = ProcessId::new(pid_num) {
                    if let Ok(info) = ProcessInfo::from_pid(pid) {
                        processes.push(info);
                    }
                }
            }
        }
    }

    Ok(processes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_kb_value() {
        assert_eq!(parse_kb_value("1024 kB"), 1024);
        assert_eq!(parse_kb_value("0 kB"), 0);
        assert_eq!(parse_kb_value("invalid"), 0);
    }

    #[test]
    fn test_get_current_process_info() {
        let current_pid = std::process::id() as i32;
        let pid = ProcessId::new(current_pid).unwrap();
        let info = ProcessInfo::from_pid(pid).unwrap();
        
        assert!(!info.name.is_empty());
        assert!(info.mem_info.vm_size > 0);
    }

    #[test]
    fn test_get_all_processes() {
        let processes = get_all_processes().unwrap();
        assert!(!processes.is_empty());
        
        // 确保至少包含当前进程
        let current_pid = std::process::id() as i32;
        assert!(processes.iter().any(|p| p.pid.as_raw() == current_pid));
    }
} 