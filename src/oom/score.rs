use std::cmp::Ordering;
use crate::linux::proc::{ProcessInfo, ProcessMemInfo};
use crate::linux::proc_stat::ProcessStat;

/// OOM 评分计算器
#[derive(Debug)]
pub struct OOMScorer {
    // 配置参数，可以通过环境变量调整
    mem_pressure_weight: f64,
    runtime_weight: f64,
    oom_score_adj_weight: f64,
}

/// 进程的 OOM 评分详情
#[derive(Debug)]
pub struct OOMScoreDetails {
    pub total_score: f64,
    pub memory_score: f64,
    pub runtime_score: f64,
    pub adj_score: f64,
    pub process: ProcessInfo,
}

impl OOMScorer {
    /// 创建新的评分器实例
    pub fn new() -> Self {
        // 从环境变量读取权重配置，使用默认值如果未设置
        let mem_pressure_weight = std::env::var("OOM_MEM_PRESSURE_WEIGHT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.6);

        let runtime_weight = std::env::var("OOM_RUNTIME_WEIGHT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.2);

        let oom_score_adj_weight = std::env::var("OOM_SCORE_ADJ_WEIGHT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.2);

        Self {
            mem_pressure_weight,
            runtime_weight,
            oom_score_adj_weight,
        }
    }

    /// 计算进程的详细评分
    /// 
    /// # 参数
    /// 
    /// * `process` - 要评分的进程信息
    /// * `total_memory` - 系统总内存大小（字节）
    /// 
    /// # 返回值
    /// 
    /// 返回包含详细评分信息的 OOMScoreDetails
    pub fn calculate_score(&self, process: ProcessInfo, total_memory: u64) -> OOMScoreDetails {
        // 计算内存压力分数 (0-1)
        let memory_score = self.calculate_memory_score(&process.mem_info, total_memory);
        
        // 计算运行时间分数 (0-1)，优先选择新进程
        let runtime_score = self.calculate_runtime_score(&process);
        
        // 计算 oom_score_adj 的影响 (-1 到 1)
        let adj_score = self.calculate_adj_score(process.mem_info.oom_score_adj);

        // 计算总分
        let total_score = 
            memory_score * self.mem_pressure_weight +
            runtime_score * self.runtime_weight +
            adj_score * self.oom_score_adj_weight;

        OOMScoreDetails {
            total_score,
            memory_score,
            runtime_score,
            adj_score,
            process,
        }
    }

    /// 计算内存压力分
    fn calculate_memory_score(&self, mem_info: &ProcessMemInfo, total_memory: u64) -> f64 {
        let rss_ratio = mem_info.vm_rss as f64 / total_memory as f64;
        let swap_ratio = mem_info.vm_swap as f64 / total_memory as f64;
        
        // RSS 使用比例和 swap 使用比例的加权和
        0.7 * rss_ratio + 0.3 * swap_ratio
    }

    /// 计算运行时间分数
    fn calculate_runtime_score(&self, process: &ProcessInfo) -> f64 {
        // 获取进程统计信息
        if let Ok(stat) = ProcessStat::from_pid(process.pid) {
            crate::linux::proc_stat::calculate_runtime_score(&stat)
        } else {
            // 如果无法获取统计信息，返回中等分数
            0.5
        }
    }

    /// 计算 oom_score_adj 的影响
    fn calculate_adj_score(&self, oom_score_adj: i32) -> f64 {
        // 将 -1000 到 1000 的范围映射到 -1 到 1
        oom_score_adj as f64 / 1000.0
    }
}

/// 为 OOMScoreDetails 实现排序
impl Ord for OOMScoreDetails {
    fn cmp(&self, other: &Self) -> Ordering {
        self.total_score.partial_cmp(&other.total_score)
            .unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for OOMScoreDetails {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for OOMScoreDetails {
    fn eq(&self, other: &Self) -> bool {
        self.total_score == other.total_score
    }
}

impl Eq for OOMScoreDetails {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::types::ProcessId;

    fn create_test_process(pid: i32, rss: u64, oom_score_adj: i32) -> ProcessInfo {
        ProcessInfo {
            pid: ProcessId::new(pid).unwrap(),
            name: format!("test_process_{}", pid),
            state: "S".to_string(),
            ppid: 1,
            mem_info: ProcessMemInfo {
                vm_peak: rss * 2,
                vm_size: rss * 2,
                vm_rss: rss,
                vm_swap: 0,
                oom_score: 0,
                oom_score_adj,
            },
        }
    }

    #[test]
    fn test_score_calculation() {
        let scorer = OOMScorer::new();
        let total_memory = 8 * 1024 * 1024 * 1024; // 8GB

        let process1 = create_test_process(1, 1024 * 1024 * 1024, 0); // 1GB RSS
        let process2 = create_test_process(2, 4 * 1024 * 1024 * 1024, 0); // 4GB RSS

        let score1 = scorer.calculate_score(process1, total_memory);
        let score2 = scorer.calculate_score(process2, total_memory);

        // 使用更多内存的进程应该得分更高
        assert!(score2.total_score > score1.total_score);
    }

    #[test]
    fn test_oom_score_adj_impact() {
        let scorer = OOMScorer::new();
        let total_memory = 8 * 1024 * 1024 * 1024;

        let process1 = create_test_process(1, 1024 * 1024 * 1024, -500);
        let process2 = create_test_process(2, 1024 * 1024 * 1024, 500);

        let score1 = scorer.calculate_score(process1, total_memory);
        let score2 = scorer.calculate_score(process2, total_memory);

        // 有更高 oom_score_adj 的进程应该得分更高
        assert!(score2.total_score > score1.total_score);
    }
} 