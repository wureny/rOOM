use std::cmp::Ordering;
use std::collections::BinaryHeap;
use crate::ffi::types::{ProcessId, SystemError, Result};
use crate::linux::proc::ProcessInfo;
use crate::oom::score::{OOMScorer, OOMScoreDetails};
use crate::oom::pressure::{PressureDetector, MemoryStats};

/// 进程选择器的配置
#[derive(Debug, Clone)]
pub struct SelectorConfig {
    /// 最小可选择进程数
    pub min_candidates: usize,
    /// 最大可选择进程数
    pub max_candidates: usize,
    /// 是否允许选择系统进程
    pub allow_system_processes: bool,
    /// 最小内存阈值（字节），小于此值的进程不会被选择
    pub min_memory_threshold: u64,
}

impl Default for SelectorConfig {
    fn default() -> Self {
        Self {
            min_candidates: 3,
            max_candidates: 10,
            allow_system_processes: false,
            min_memory_threshold: 1024 * 1024, // 1MB
        }
    }
}

/// 进程选择器
#[derive(Debug)]
pub struct ProcessSelector {
    config: SelectorConfig,
    scorer: OOMScorer,
    pressure_detector: PressureDetector,
}

/// 候选进程信息
#[derive(Debug)]
pub struct Candidate {
    pub score_details: OOMScoreDetails,
    pub memory_saved: u64,
}

impl ProcessSelector {
    /// 创建新的进程选择器
    pub fn new(
        config: Option<SelectorConfig>,
        scorer: OOMScorer,
        pressure_detector: PressureDetector,
    ) -> Self {
        Self {
            config: config.unwrap_or_default(),
            scorer,
            pressure_detector,
        }
    }

    /// 选择最适合终止的进程
    pub fn select_process(&mut self) -> Result<Option<ProcessId>> {
        // 检查系统是否真的处于内存压力状态
        if !self.pressure_detector.check_pressure()? {
            return Ok(None);
        }

        // 获取内存统计信息
        let memory_stats = self.pressure_detector.get_memory_stats()?;
        
        // 获取并评分所有可能的候选进程
        let candidates = self.get_candidates(&memory_stats)?;
        
        // 如果没有足够的候选进程，返回None
        if candidates.len() < self.config.min_candidates {
            return Ok(None);
        }

        // 选择得分最高的进程
        Ok(candidates.into_iter()
            .max_by_key(|c| OrderedFloat(c.score_details.total_score))
            .map(|c| c.score_details.process.pid))
    }

    /// 获取所有候选进程
    fn get_candidates(&self, memory_stats: &MemoryStats) -> Result<Vec<Candidate>> {
        let mut candidates = BinaryHeap::new();
        let processes = crate::linux::proc::get_all_processes()?;

        for process in processes {
            if self.is_valid_candidate(&process, memory_stats) {
                let score_details = self.scorer.calculate_score(
                    process.clone(),
                    memory_stats.total_memory
                );

                let memory_saved = process.mem_info.vm_rss;
                
                candidates.push(Candidate {
                    score_details,
                    memory_saved,
                });

                // 限制候选进程数量
                if candidates.len() > self.config.max_candidates {
                    candidates.pop();
                }
            }
        }

        Ok(candidates.into_sorted_vec())
    }

    /// 检查进程是否是有效的候选者
    fn is_valid_candidate(&self, process: &ProcessInfo, memory_stats: &MemoryStats) -> bool {
        // 检查是否是系统进程
        if !self.config.allow_system_processes && process.is_system_process() {
            return false;
        }

        // 检查内存使用是否达到最小阈值
        if process.mem_info.vm_rss < self.config.min_memory_threshold {
            return false;
        }

        // 检查进程是否可以被OOM killer终止
        if !process.is_oomable() {
            return false;
        }

        // 检查终止该进程是否能显著改善内存状况
        let memory_impact = process.mem_info.vm_rss as f64 / memory_stats.total_memory as f64;
        memory_impact >= 0.01 // 至少释放1%的系统内存
    }

    /// 获取选择器的当前状态信息
    pub fn get_status(&self) -> Result<SelectorStatus> {
        let pressure_info = self.pressure_detector.get_pressure_info()?;
        
        Ok(SelectorStatus {
            memory_stats: pressure_info.stats,
            pressure_duration: pressure_info.pressure_duration,
            last_check: pressure_info.last_check,
        })
    }
}

/// 用于比较浮点数的包装类型
#[derive(Debug, Copy, Clone, PartialEq)]
struct OrderedFloat(f64);

impl Eq for OrderedFloat {}

impl PartialOrd for OrderedFloat {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl Ord for OrderedFloat {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap_or(Ordering::Equal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_process_selection() {
        let config = SelectorConfig {
            min_candidates: 1,
            max_candidates: 5,
            allow_system_processes: false,
            min_memory_threshold: 1024 * 1024,
        };

        let scorer = OOMScorer::new();
        let pressure_detector = PressureDetector::new(None);
        let mut selector = ProcessSelector::new(
            Some(config),
            scorer,
            pressure_detector
        );

        // 测试进程选择
        match selector.select_process() {
            Ok(Some(pid)) => {
                // 验证选中的进程
                let process = ProcessInfo::from_pid(pid).unwrap();
                assert!(process.mem_info.vm_rss >= 1024 * 1024);
                assert!(process.is_oomable());
            }
            Ok(None) => {
                // 系统可能没有处于内存压力状态
                println!("No process selected (system might not be under memory pressure)");
            }
            Err(e) => panic!("Process selection failed: {:?}", e),
        }
    }

    #[test]
    fn test_candidate_filtering() {
        let config = SelectorConfig::default();
        let scorer = OOMScorer::new();
        let pressure_detector = PressureDetector::new(None);
        let selector = ProcessSelector::new(
            Some(config),
            scorer,
            pressure_detector
        );

        let memory_stats = MemoryStats {
            total_memory: 8 * 1024 * 1024 * 1024, // 8GB
            free_memory: 4 * 1024 * 1024 * 1024,  // 4GB
            available_memory: 4 * 1024 * 1024 * 1024,
            total_swap: 1024 * 1024 * 1024,
            free_swap: 512 * 1024 * 1024,
            cached_memory: 1024 * 1024 * 1024,
        };

        // 创建测试进程
        let test_process = ProcessInfo::new_test(
            ProcessId::new(1).unwrap(),
            "test",
            2 * 1024 * 1024 * 1024, // 2GB RSS
            0
        );

        assert!(selector.is_valid_candidate(&test_process, &memory_stats));
    }
} 