use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use crate::ffi::types::{ProcessId, SystemError, Result};
use crate::oom::score::OOMScorer;
use crate::oom::pressure::{PressureDetector, PressureThresholds};
use crate::oom::selector::{ProcessSelector, SelectorConfig};
use std::thread;

/// OOM Killer的配置
#[derive(Debug, Clone)]
pub struct KillerConfig {
    /// 选择器配置
    pub selector: SelectorConfig,
    /// 内存压力阈值配置
    pub pressure: PressureThresholds,
    /// 两次终止进程之间的最小间隔
    pub min_kill_interval: Duration,
    /// 检查内存压力的间隔
    pub check_interval: Duration,
}

impl Default for KillerConfig {
    fn default() -> Self {
        Self {
            selector: SelectorConfig::default(),
            pressure: PressureThresholds::default(),
            min_kill_interval: Duration::from_secs(5),
            check_interval: Duration::from_millis(100),
        }
    }
}

/// OOM Killer的运行状态
#[derive(Debug, Clone)]
pub struct KillerStatus {
    pub last_kill_time: Option<Instant>,
    pub total_kills: u64,
    pub total_memory_reclaimed: u64,
    pub running_since: Instant,
}

/// OOM Killer的主要实现
pub struct OOMKiller {
    config: KillerConfig,
    selector: ProcessSelector,
    running: Arc<AtomicBool>,
    last_kill_time: Option<Instant>,
    total_kills: u64,
    total_memory_reclaimed: u64,
    running_since: Instant,
}

impl OOMKiller {
    /// 创建新的OOM Killer实例
    pub fn new(config: Option<KillerConfig>) -> Self {
        let config = config.unwrap_or_default();
        let scorer = OOMScorer::new();
        let pressure_detector = PressureDetector::new(Some(config.pressure.clone()));
        let selector = ProcessSelector::new(
            Some(config.selector.clone()),
            scorer,
            pressure_detector,
        );

        Self {
            config,
            selector,
            running: Arc::new(AtomicBool::new(false)),
            last_kill_time: None,
            total_kills: 0,
            total_memory_reclaimed: 0,
            running_since: Instant::now(),
        }
    }

    /// 启动OOM Killer
    pub fn start(&mut self) -> Result<()> {
        if self.running.load(Ordering::SeqCst) {
            return Ok(());
        }

        self.running.store(true, Ordering::SeqCst);
        let running = Arc::clone(&self.running);
        let config = self.config.clone();

        // 在新线程中运行监控循环
        thread::Builder::new()
            .name("oom-killer".to_string())
            .spawn(move || {
                let mut killer = OOMKiller::new(Some(config));
                while running.load(Ordering::SeqCst) {
                    if let Err(e) = killer.check_and_kill() {
                        eprintln!("OOM Killer error: {:?}", e);
                    }
                    thread::sleep(killer.config.check_interval);
                }
            })
            .map_err(|e| SystemError::SyscallError(e))?;

        Ok(())
    }

    /// 停止OOM Killer
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// 检查内存状态并在必要时终止进程
    fn check_and_kill(&mut self) -> Result<()> {
        // 检查是否需要等待kill间隔
        if let Some(last_time) = self.last_kill_time {
            if last_time.elapsed() < self.config.min_kill_interval {
                return Ok(());
            }
        }

        // 选择进程
        if let Some(pid) = self.selector.select_process()? {
            // 获取进程信息（用于记录）
            let process = crate::linux::proc::ProcessInfo::from_pid(pid)?;
            let memory_freed = process.mem_info.vm_rss;

            // 终止进程
            self.kill_process(pid)?;

            // 更新统计信息
            self.last_kill_time = Some(Instant::now());
            self.total_kills += 1;
            self.total_memory_reclaimed += memory_freed;

            // 记录操作
            self.log_kill(&process);
        }

        Ok(())
    }

    /// 终止指定的进程
    fn kill_process(&self, pid: ProcessId) -> Result<()> {
        use crate::ffi::safe_wrapper::SystemInterface;
        
        let system = SystemInterface::new();
        // 发送SIGKILL信号
        system.kill(pid, libc::SIGKILL)
    }

    /// 记录终止进程的操作
    fn log_kill(&self, process: &crate::linux::proc::ProcessInfo) {
        // TODO: 实现更好的日志系统
        println!(
            "OOM Killer terminated process {} ({}), freed {} MB of memory",
            process.pid.as_raw(),
            process.name,
            process.mem_info.vm_rss / 1024 / 1024
        );
    }

    /// 获取当前状态
    pub fn get_status(&self) -> KillerStatus {
        KillerStatus {
            last_kill_time: self.last_kill_time,
            total_kills: self.total_kills,
            total_memory_reclaimed: self.total_memory_reclaimed,
            running_since: self.running_since,
        }
    }
}

/// 用于测试的模拟进程终止器
#[cfg(test)]
pub struct MockKiller {
    killed_processes: Vec<ProcessId>,
}

#[cfg(test)]
impl MockKiller {
    pub fn new() -> Self {
        Self {
            killed_processes: Vec::new(),
        }
    }

    pub fn kill(&mut self, pid: ProcessId) -> Result<()> {
        self.killed_processes.push(pid);
        Ok(())
    }

    pub fn get_killed_processes(&self) -> &[ProcessId] {
        &self.killed_processes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_oom_killer_lifecycle() {
        let mut killer = OOMKiller::new(None);
        
        // 测试启动
        assert!(killer.start().is_ok());
        assert!(killer.running.load(Ordering::SeqCst));

        // 等待一段时间
        thread::sleep(Duration::from_secs(1));

        // 测试停止
        killer.stop();
        assert!(!killer.running.load(Ordering::SeqCst));

        // 验证状态
        let status = killer.get_status();
        assert!(status.running_since <= Instant::now());
    }

    #[test]
    fn test_kill_interval() {
        let config = KillerConfig {
            min_kill_interval: Duration::from_millis(100),
            ..Default::default()
        };

        let mut killer = OOMKiller::new(Some(config));
        
        // 第一次检查应该可以执行
        assert!(killer.check_and_kill().is_ok());

        // 立即再次检查应该被间隔限制
        if let Some(last_time) = killer.last_kill_time {
            assert!(last_time.elapsed() < killer.config.min_kill_interval);
        }
    }

    #[test]
    fn test_mock_killer() {
        let mut mock = MockKiller::new();
        let pid = ProcessId::new(1234).unwrap();

        assert!(mock.kill(pid).is_ok());
        assert_eq!(mock.get_killed_processes(), &[pid]);
    }
} 