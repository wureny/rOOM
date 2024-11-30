use std::time::{Duration, Instant};
use crate::ffi::types::{SystemError, Result};
use crate::linux::proc::ProcessInfo;
use std::fs::File;
use std::io::{self, BufRead, BufReader};

/// 内存压力阈值配置
#[derive(Debug, Clone)]
pub struct PressureThresholds {
    /// 可用内存占总内存的最小比例（0-1）
    pub min_free_ratio: f64,
    /// swap使用率的最大比例（0-1）
    pub max_swap_ratio: f64,
    /// 内存压力持续时间阈值
    pub pressure_duration: Duration,
}

impl Default for PressureThresholds {
    fn default() -> Self {
        Self {
            min_free_ratio: 0.05,  // 5%可用内存
            max_swap_ratio: 0.80,  // 80% swap使用率
            pressure_duration: Duration::from_secs(5),
        }
    }
}

/// 内存压力检测器
#[derive(Debug)]
pub struct PressureDetector {
    thresholds: PressureThresholds,
    pressure_start: Option<Instant>,
    last_pressure_check: Instant,
}

/// 内存统计信息
#[derive(Debug, Clone)]
pub struct MemoryStats {
    pub total_memory: u64,
    pub free_memory: u64,
    pub available_memory: u64,
    pub total_swap: u64,
    pub free_swap: u64,
    pub cached_memory: u64,
}

impl PressureDetector {
    /// 创建新的压力检测器实例
    pub fn new(thresholds: Option<PressureThresholds>) -> Self {
        Self {
            thresholds: thresholds.unwrap_or_default(),
            pressure_start: None,
            last_pressure_check: Instant::now(),
        }
    }

    /// 检查系统是否处于内存压力状态
    /// 
    /// # 返回值
    /// 
    /// 如果系统处于持续的内存压力状态，返回 true
    pub fn check_pressure(&mut self) -> Result<bool> {
        let stats = self.get_memory_stats()?;
        let now = Instant::now();

        // 计算关键指标
        let free_ratio = stats.available_memory as f64 / stats.total_memory as f64;
        let swap_used_ratio = if stats.total_swap > 0 {
            (stats.total_swap - stats.free_swap) as f64 / stats.total_swap as f64
        } else {
            0.0
        };

        // 判断是否处于压力状态
        let under_pressure = free_ratio < self.thresholds.min_free_ratio || 
                           swap_used_ratio > self.thresholds.max_swap_ratio;

        // 更新压力状态
        if under_pressure {
            if self.pressure_start.is_none() {
                self.pressure_start = Some(now);
            }
            
            // 检查压力持续时间
            if now.duration_since(self.pressure_start.unwrap()) >= self.thresholds.pressure_duration {
                return Ok(true);
            }
        } else {
            self.pressure_start = None;
        }

        self.last_pressure_check = now;
        Ok(false)
    }

    /// 获取当前内存统计信息
    pub fn get_memory_stats(&self) -> Result<MemoryStats> {
        let file = File::open("/proc/meminfo").map_err(|e| 
            SystemError::SyscallError(e)
        )?;

        let reader = BufReader::new(file);
        let mut stats = MemoryStats {
            total_memory: 0,
            free_memory: 0,
            available_memory: 0,
            total_swap: 0,
            free_swap: 0,
            cached_memory: 0,
        };

        for line in reader.lines() {
            let line = line?;
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                continue;
            }

            let value = parts[1].parse::<u64>().unwrap_or(0) * 1024; // 转换为字节
            match parts[0] {
                "MemTotal:" => stats.total_memory = value,
                "MemFree:" => stats.free_memory = value,
                "MemAvailable:" => stats.available_memory = value,
                "SwapTotal:" => stats.total_swap = value,
                "SwapFree:" => stats.free_swap = value,
                "Cached:" => stats.cached_memory = value,
                _ => {}
            }
        }

        Ok(stats)
    }

    /// 获取系统内存压力的详细信息
    pub fn get_pressure_info(&self) -> Result<PressureInfo> {
        let stats = self.get_memory_stats()?;
        
        Ok(PressureInfo {
            stats,
            pressure_duration: self.pressure_start
                .map(|start| start.elapsed())
                .unwrap_or_default(),
            last_check: self.last_pressure_check.elapsed(),
        })
    }
}

/// 内存压力详细信息
#[derive(Debug)]
pub struct PressureInfo {
    pub stats: MemoryStats,
    pub pressure_duration: Duration,
    pub last_check: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_memory_stats() {
        let detector = PressureDetector::new(None);
        let stats = detector.get_memory_stats().unwrap();
        
        // 验证基本的内存统计信息
        assert!(stats.total_memory > 0);
        assert!(stats.available_memory <= stats.total_memory);
        assert!(stats.free_memory <= stats.total_memory);
    }

    #[test]
    fn test_pressure_detection() {
        let mut detector = PressureDetector::new(Some(PressureThresholds {
            min_free_ratio: 0.99, // 设置一个极高的阈值来模拟压力
            max_swap_ratio: 0.0,
            pressure_duration: Duration::from_millis(100),
        }));

        // 第一次检查应该开始计时但不触发
        assert!(!detector.check_pressure().unwrap());

        // 等待足够长的时间
        thread::sleep(Duration::from_millis(150));

        // 第二次检查应该触发压力警告
        assert!(detector.check_pressure().unwrap());
    }

    #[test]
    fn test_pressure_recovery() {
        let mut detector = PressureDetector::new(Some(PressureThresholds {
            min_free_ratio: 0.0, // 设置一个极低的阈值
            max_swap_ratio: 1.0,
            pressure_duration: Duration::from_millis(100),
        }));

        // 在正常阈值下不应该检测到压力
        assert!(!detector.check_pressure().unwrap());
        
        // 压力开始时间应该被重置
        assert!(detector.pressure_start.is_none());
    }
} 