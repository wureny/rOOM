use std::fs::File;
use std::io::{self, Read};
use std::time::Duration;
use crate::ffi::types::{ProcessId, SystemError, Result};

/// 进程的统计信息
#[derive(Debug, Clone)]
pub struct ProcessStat {
    pub pid: ProcessId,
    pub comm: String,
    pub state: char,
    pub ppid: i32,
    pub start_time: u64,     // 进程启动时间（自系统启动以来的时钟滴答数）
    pub utime: u64,          // 用户态CPU时间
    pub stime: u64,          // 内核态CPU时间
    pub cutime: u64,         // 子进程用户态CPU时间
    pub cstime: u64,         // 子进程内核态CPU时间
}

impl ProcessStat {
    /// 从/proc/[pid]/stat获取进程统计信息
    pub fn from_pid(pid: ProcessId) -> Result<Self> {
        let path = format!("/proc/{}/stat", pid.as_raw());
        let mut content = String::new();
        File::open(&path)
            .and_then(|mut file| file.read_to_string(&mut content))
            .map_err(|e| {
                if e.kind() == io::ErrorKind::NotFound {
                    SystemError::ProcessNotFound
                } else {
                    SystemError::SyscallError(e)
                }
            })?;

        Self::parse_stat(&content, pid)
    }

    /// 解析stat文件内容
    fn parse_stat(content: &str, pid: ProcessId) -> Result<Self> {
        // stat文件格式较复杂，特别是进程名可能包含空格和括号
        let mut parts: Vec<&str> = content.split_whitespace().collect();
        
        // 确保至少有最小数量的字段
        if parts.len() < 24 {
            return Err(SystemError::SyscallError(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid stat file format"
            )));
        }

        // 处理进程名（可能包含空格）
        let comm_start = content.find('(').ok_or_else(|| {
            SystemError::SyscallError(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid process name format"
            ))
        })?;
        let comm_end = content.rfind(')').ok_or_else(|| {
            SystemError::SyscallError(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid process name format"
            ))
        })?;
        let comm = content[comm_start + 1..comm_end].to_string();

        // 重新分割剩余部分
        let remainder = &content[comm_end + 1..];
        parts = remainder.split_whitespace().collect();

        Ok(ProcessStat {
            pid,
            comm,
            state: parts[0].chars().next().unwrap_or('?'),
            ppid: parts[2].parse().unwrap_or(0),
            utime: parts[11].parse().unwrap_or(0),
            stime: parts[12].parse().unwrap_or(0),
            cutime: parts[13].parse().unwrap_or(0),
            cstime: parts[14].parse().unwrap_or(0),
            start_time: parts[19].parse().unwrap_or(0),
        })
    }

    /// 获取进程的总CPU时间
    pub fn total_cpu_time(&self) -> Duration {
        let ticks = self.utime + self.stime + self.cutime + self.cstime;
        // 将时钟滴答数转换为Duration
        // 通常Linux的时钟频率是100Hz，即每秒100个时钟滴答
        Duration::from_secs_f64(ticks as f64 / 100.0)
    }

    /// 获取进程的运行时长
    pub fn running_time(&self) -> Duration {
        // 读取系统启动时间
        let uptime = Self::get_system_uptime()
            .unwrap_or_else(|_| Duration::from_secs(0));
        
        // 计算进程运行时间
        let process_uptime = Duration::from_secs_f64(
            self.start_time as f64 / 100.0  // 转换启动时间的时钟滴答数
        );
        
        uptime.saturating_sub(process_uptime)
    }

    /// 获取系统运行时间
    fn get_system_uptime() -> Result<Duration> {
        let mut content = String::new();
        File::open("/proc/uptime")
            .and_then(|mut file| file.read_to_string(&mut content))
            .map_err(SystemError::SyscallError)?;

        let uptime: f64 = content
            .split_whitespace()
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);

        Ok(Duration::from_secs_f64(uptime))
    }
}

/// 现在我们可以更新 OOMScorer 中的 calculate_runtime_score 方法
pub fn calculate_runtime_score(process_stat: &ProcessStat) -> f64 {
    const HOUR: u64 = 3600;
    const DAY: u64 = HOUR * 24;
    
    let runtime = process_stat.running_time();
    let runtime_secs = runtime.as_secs();

    // 根据运行时间计算分数：
    // - 运行时间很短的进程（<1小时）得分较高
    // - 运行时间适中的进程（1小时-1天）得分适中
    // - 运行时间很长的进程（>1天）得分较低
    if runtime_secs < HOUR {
        // 新进程，得分从0.8到1.0
        0.8 + (0.2 * (HOUR - runtime_secs) as f64 / HOUR as f64)
    } else if runtime_secs < DAY {
        // 中等时间的进程，得分从0.3到0.8
        0.3 + (0.5 * (DAY - runtime_secs) as f64 / DAY as f64)
    } else {
        // 长期运行的进程，得分从0.0到0.3
        0.3 * (2.0 * DAY - runtime_secs.min(2 * DAY)) as f64 / DAY as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_read_current_process_stat() {
        let pid = ProcessId::new(std::process::id() as i32).unwrap();
        let stat = ProcessStat::from_pid(pid).unwrap();
        
        assert_eq!(stat.pid, pid);
        assert!(!stat.comm.is_empty());
        assert!(stat.start_time > 0);
    }

    #[test]
    fn test_process_times() {
        let pid = ProcessId::new(std::process::id() as i32).unwrap();
        let stat = ProcessStat::from_pid(pid).unwrap();
        
        let cpu_time = stat.total_cpu_time();
        let running_time = stat.running_time();
        
        assert!(running_time > Duration::from_secs(0));
        assert!(cpu_time <= running_time);
    }

    #[test]
    fn test_runtime_score() {
        let pid = ProcessId::new(std::process::id() as i32).unwrap();
        let stat = ProcessStat::from_pid(pid).unwrap();
        
        let score = calculate_runtime_score(&stat);
        assert!(score >= 0.0 && score <= 1.0);
    }

    #[test]
    fn test_runtime_score_values() {
        // 模拟不同运行时间的进程统计信息
        let mut stat = ProcessStat {
            pid: ProcessId::new(1).unwrap(),
            comm: String::from("test"),
            state: 'R',
            ppid: 0,
            start_time: 0,
            utime: 0,
            stime: 0,
            cutime: 0,
            cstime: 0,
        };

        // 测试新进程（运行时间小于1小时）
        stat.start_time = (Duration::from_secs(1800).as_secs() * 100) as u64; // 30分钟
        let new_process_score = calculate_runtime_score(&stat);
        
        // 测试中等时间进程（运行时间在1小时到1天之间）
        stat.start_time = (Duration::from_secs(12 * 3600).as_secs() * 100) as u64; // 12小时
        let medium_process_score = calculate_runtime_score(&stat);
        
        // 测试长期运行进程（运行时间超过1天）
        stat.start_time = (Duration::from_secs(2 * 24 * 3600).as_secs() * 100) as u64; // 2天
        let long_process_score = calculate_runtime_score(&stat);

        // 验证分数范围和相对大小
        assert!(new_process_score > medium_process_score);
        assert!(medium_process_score > long_process_score);
    }
} 