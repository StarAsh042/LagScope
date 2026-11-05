//! PDH(Windows 性能计数器)封装。
//!
//! 统一使用 `PdhAddEnglishCounterW` 添加计数器:中/英文等本地化系统上
//! 本地化计数器名(`\处理器(_Total)\% 处理器时间`)会失效,English 名始终可用。

use windows::core::{HSTRING, PCWSTR, PWSTR};

use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhGetFormattedCounterValue, PdhOpenQueryW, PDH_FMT_COUNTERVALUE, PDH_FMT_COUNTERVALUE_ITEM_W,
    PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
};

/// PDH_CSTATUS_VALID_DATA / PDH_CSTATUS_NEW_DATA 都表示有效数据
const PDH_CSTATUS_VALID_DATA: u32 = 0;
const PDH_CSTATUS_NEW_DATA: u32 = 1;
/// 缓冲区不够大,需按返回的大小重试(0x800007D2)
const PDH_MORE_DATA: u32 = 0x8000_07D2;

/// 计数器句柄别名
pub type Counter = PDH_HCOUNTER;

fn check(status: u32, ctx: &str) -> Result<(), String> {
    if status == 0 {
        Ok(())
    } else {
        Err(format!("PDH {ctx} 失败: 0x{status:08X}"))
    }
}

pub struct PdhQuery {
    query: PDH_HQUERY,
}

impl PdhQuery {
    /// 打开查询。首个有效采样前需调用两次 [`Self::collect`]。
    pub fn new() -> Result<Self, String> {
        let mut query = PDH_HQUERY::default();
        check(
            unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut query) },
            "open query",
        )?;
        Ok(Self { query })
    }

    /// 添加 English 计数器路径。
    pub fn add(&self, path: &str) -> Result<Counter, String> {
        let h = HSTRING::from(path);
        let mut counter = PDH_HCOUNTER::default();
        check(
            unsafe { PdhAddEnglishCounterW(self.query, PCWSTR(h.as_ptr()), 0, &mut counter) },
            &format!("add {path}"),
        )?;
        Ok(counter)
    }

    pub fn collect(&self) -> Result<(), String> {
        check(unsafe { PdhCollectQueryData(self.query) }, "collect")
    }

    /// 读取单个(double 型)计数器值;状态无效返回 None。
    pub fn read(&self, counter: &Counter) -> Option<f64> {
        let mut value = PDH_FMT_COUNTERVALUE::default();
        let status = unsafe {
            PdhGetFormattedCounterValue(*counter, PDH_FMT_DOUBLE, None, &mut value)
        };
        if status != 0 {
            return None;
        }
        if value.CStatus != PDH_CSTATUS_VALID_DATA && value.CStatus != PDH_CSTATUS_NEW_DATA {
            return None;
        }
        Some(unsafe { value.Anonymous.doubleValue })
    }

    /// 读取通配符计数器(如 `\GPU Engine(*)\...`)的全部实例。
    /// 返回 (实例名, 值) 列表。
    pub fn read_instances(&self, counter: &Counter) -> Vec<(String, f64)> {
        let mut size = 0u32;
        let mut count = 0u32;
        let status = unsafe {
            PdhGetFormattedCounterArrayW(*counter, PDH_FMT_DOUBLE, &mut size, &mut count, None)
        };
        if status != PDH_MORE_DATA && status != 0 {
            return Vec::new();
        }
        if size == 0 {
            return Vec::new();
        }
        // PWSTR 字段 8 字节对齐,用 u64 数组做缓冲
        let mut buf = vec![0u64; size.div_ceil(8) as usize];
        let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
        let status = unsafe {
            PdhGetFormattedCounterArrayW(
                *counter,
                PDH_FMT_DOUBLE,
                &mut size,
                &mut count,
                Some(items),
            )
        };
        if status != 0 {
            return Vec::new();
        }
        let slice = unsafe { std::slice::from_raw_parts(items, count as usize) };
        slice
            .iter()
            .filter(|it| {
                it.FmtValue.CStatus == PDH_CSTATUS_VALID_DATA
                    || it.FmtValue.CStatus == PDH_CSTATUS_NEW_DATA
            })
            .map(|it| {
                let name = PWSTR(it.szName.as_ptr());
                let name = unsafe { name.to_string() }.unwrap_or_default();
                (name, unsafe { it.FmtValue.Anonymous.doubleValue })
            })
            .collect()
    }
}

impl Drop for PdhQuery {
    fn drop(&mut self) {
        unsafe { PdhCloseQuery(self.query) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn pdh_counters_smoke() {
        let q = PdhQuery::new().expect("open query");
        let cpu = q.add(r"\Processor(_Total)\% Processor Time").expect("add cpu");
        let dpc = q
            .add(r"\Processor(_Total)\DPCs Queued/sec")
            .or_else(|_| q.add(r"\Processor(_Total)\DPCs/sec"))
            .expect("add dpc");
        let gpu = q.add(r"\GPU Engine(*engtype_3D)\Utilization Percentage")
            .expect("add gpu engine");

        q.collect().expect("collect #1");
        sleep(Duration::from_millis(300));
        q.collect().expect("collect #2");

        let cpu_v = q.read(&cpu).expect("cpu value");
        assert!((0.0..=100.0).contains(&cpu_v), "cpu% 超范围: {cpu_v}");
        // DPC 速率可能为 0,只验证读取不 panic
        let _ = q.read(&dpc);
        // GPU 计数器在无独显/精简系统上实例可能为空,仅验证 API 可调
        let _ = q.read_instances(&gpu);
    }
}
