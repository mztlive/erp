//! 历史回填进度折叠值对象（INT-E08）。
//!
//! `submit_backfill_command` 的五类计数器与重复进度折叠曾由 Service 手工维护，
//! 现由本值对象独占：调用方按稳定的事实顺序逐项上报重复或分类结果，本对象以
//! `checked_add` 累加并在收口时一次性产出作业进度更新。时钟、ID 与持久化仍由
//! Service 持有；本对象不访问数据库、HTTP 或全局时钟。

use super::backfill_job::{BackfillCostBasis, BackfillItemClassification, BackfillItemResult};
use crate::errors::{Error, Result};
use crate::mall_backfill::MallConsumptionBackfillJob;

/// 历史回填进度累加器（INT-E08）。
///
/// 计数口径沿用作业 `update_progress` 的五字段语义：重叠去重、实际／标准／无
/// 成本笔数与未归集数量。成本口径按分类原样累计（含 `Failed` 结果的 `None`
/// 口径，保持作业视图语义不变）；`failed` 单独计数，供后台任务的失败口径使用。
/// `succeeded + failed + deduplicated` 恒等于上报总数；`deduplicated` 同时覆盖
/// 库内重复与请求内重复。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BackfillProgress {
    /// 重叠去重数量。
    deduplicated: u64,
    /// 实际成本口径笔数。
    actual: u64,
    /// 标准成本口径笔数。
    standard: u64,
    /// 无成本口径笔数（含 `Failed` 结果，作业视图语义不变）。
    none: u64,
    /// 未归集数量。
    unattributed: u64,
    /// 失败结果数量（`BackfillItemResult::Failed`）。
    failed: u64,
}

impl BackfillProgress {
    /// 创建空进度累加器。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回全部计数为零的累加器。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 纯内存构造，不访问 I/O；计数上溢由每次记录返回错误。
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一项重复（库内已存在或请求内重复业务键）。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 成功返回 `Ok(())`；累加器保持可继续上报。
    ///
    /// # 错误
    /// 去重计数上溢时返回错误，累加器保持原值。
    ///
    /// # 约束
    /// 重复项不进入任何成本口径；与逐条预查的去重语义一致。
    pub fn record_duplicate(&mut self) -> Result<()> {
        self.deduplicated = self
            .deduplicated
            .checked_add(1)
            .ok_or_else(|| Error::from("回填去重计数溢出"))?;
        Ok(())
    }

    /// 按确定性分类记录一项新增明细。
    ///
    /// # 参数
    /// * `classification` - 已由 `BackfillItemClassification::from_mall_fact` 派生的分类
    ///
    /// # 返回
    /// 成功返回 `Ok(())`；成本口径、未归集与失败计数同步推进。
    ///
    /// # 错误
    /// 任一计数上溢时返回错误，累加器保持原值。
    ///
    /// # 约束
    /// 成本口径与 `result` 的映射固定：`Actual`／`Standard`／`None` 各计一处
    /// （`Failed` 结果的 `None` 口径一并计入，保持作业视图语义不变）；
    /// `PendingAttribution` 额外计未归集；`Failed` 额外计失败；不猜测成本、不做 I/O。
    pub fn record_item(&mut self, classification: BackfillItemClassification) -> Result<()> {
        let next_actual = self.actual;
        let next_standard = self.standard;
        let next_none = self.none;
        let next_unattributed = self.unattributed;
        let next_failed = self.failed;
        let (next_actual, next_standard, next_none) = match classification.cost_basis {
            BackfillCostBasis::Actual => (bump(next_actual)?, next_standard, next_none),
            BackfillCostBasis::Standard => (next_actual, bump(next_standard)?, next_none),
            BackfillCostBasis::None => (next_actual, next_standard, bump(next_none)?),
        };
        let next_unattributed = if classification.result == BackfillItemResult::PendingAttribution {
            bump(next_unattributed)?
        } else {
            next_unattributed
        };
        let next_failed = if classification.result == BackfillItemResult::Failed {
            bump(next_failed)?
        } else {
            next_failed
        };
        self.actual = next_actual;
        self.standard = next_standard;
        self.none = next_none;
        self.unattributed = next_unattributed;
        self.failed = next_failed;
        Ok(())
    }

    /// 返回重叠去重数量。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回累计的去重数。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 纯读取；与后台任务 `skipped` 口径一致。
    pub fn deduplicated(&self) -> u64 {
        self.deduplicated
    }

    /// 返回已创建明细中非失败结果的总数（`actual + standard + none - failed`）。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回三类成本口径之和减去失败数。
    ///
    /// # 错误
    /// 无（每次记录均保证成本口径合计不小于失败数、不溢出）。
    ///
    /// # 约束
    /// 纯读取；与后台任务 `success` 口径一致，`Failed` 结果改走 `failed` 口径。
    pub fn succeeded(&self) -> u64 {
        self.actual + self.standard + self.none - self.failed
    }

    /// 返回实际成本口径笔数。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回实际成本计数。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 纯读取。
    pub fn actual(&self) -> u64 {
        self.actual
    }

    /// 返回标准成本口径笔数。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回标准成本计数。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 纯读取。
    pub fn standard(&self) -> u64 {
        self.standard
    }

    /// 返回无成本口径笔数。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回无成本计数。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 纯读取。
    pub fn none(&self) -> u64 {
        self.none
    }

    /// 返回未归集数量。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回未归集计数。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 纯读取。
    pub fn unattributed(&self) -> u64 {
        self.unattributed
    }

    /// 返回失败结果数量。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `Failed` 结果计数。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 纯读取；与后台任务 `failed` 口径一致。`succeeded + failed + deduplicated`
    /// 恒等于上报总数，满足 `mark_succeeded` 的 `processed == total` 不变量。
    pub fn failed(&self) -> u64 {
        self.failed
    }

    /// 把累计进度写入回填作业。
    ///
    /// # 参数
    /// * `job` - 待更新的回填作业（调用方事务内持有）
    ///
    /// # 返回
    /// 成功返回 `Ok(())`；作业五项计数与本累加器一致。
    ///
    /// # 错误
    /// 作业已完成、合计超过来源总笔数时返回错误（复用作业校验）。
    ///
    /// # 约束
    /// 仅调用作业的纯 `update_progress`；报告文件引用保持不修改。
    pub fn apply_to_job(&self, job: &mut MallConsumptionBackfillJob) -> Result<()> {
        job.update_progress(
            self.deduplicated,
            self.actual,
            self.standard,
            self.none,
            self.unattributed,
            None,
        )
    }
}

/// 计数加一并在上溢时返回领域错误。
///
/// # 参数
/// * `value` - 当前计数值
///
/// # 返回
/// 返回加一后的计数。
///
/// # 错误
/// 已为 `u64::MAX` 时返回计数溢出错误。
///
/// # 约束
/// 纯算术；失败时调用方累加器保持原值。
fn bump(value: u64) -> Result<u64> {
    value
        .checked_add(1)
        .ok_or_else(|| Error::from("回填进度计数溢出"))
}

#[cfg(test)]
mod tests {
    use super::BackfillProgress;
    use crate::common::time::Instant;
    use crate::ids::{MallConsumptionBackfillJobId, MallConsumptionCutoverId};
    use crate::mall_backfill::{
        BackfillCostBasis, BackfillItemClassification, BackfillItemResult, MallConsumptionBackfillJob,
        MallConsumptionBackfillJobData,
    };
    use crate::mall_order::{FactType, ProcessingStatus};
    use crate::money::Amount;
    use std::str::FromStr;

    /// happy path：空累加器五项均为零。
    #[test]
    fn new_starts_at_zero() {
        let progress = BackfillProgress::new();
        assert_eq!(progress.deduplicated(), 0);
        assert_eq!(progress.succeeded(), 0);
        assert_eq!(progress.unattributed(), 0);
    }

    /// happy path：全部 deterministic 分类的口径映射。
    #[test]
    fn record_item_covers_all_classifications() {
        let mut progress = BackfillProgress::new();
        let attributed_payment = BackfillItemClassification::from_mall_fact(
            FactType::PaymentSucceeded,
            ProcessingStatus::Attributed,
        );
        assert_eq!(attributed_payment.result, BackfillItemResult::New);
        progress.record_item(attributed_payment).unwrap();

        let pending = BackfillItemClassification::from_mall_fact(
            FactType::OrderCompleted,
            ProcessingStatus::PendingAttribution,
        );
        progress.record_item(pending).unwrap();
        progress.record_duplicate().unwrap();

        assert_eq!(progress.actual(), 1);
        assert_eq!(progress.none(), 1);
        assert_eq!(progress.unattributed(), 1);
        assert_eq!(progress.deduplicated(), 1);
        assert_eq!(progress.failed(), 0);
        assert_eq!(progress.succeeded(), 2);
    }

    /// 边界：重复项不进入成本口径；总数恒等于三口径之和。
    #[test]
    fn duplicates_do_not_touch_cost_basis() {
        let mut progress = BackfillProgress::new();
        progress.record_duplicate().unwrap();
        progress.record_duplicate().unwrap();
        assert_eq!(progress.succeeded(), 0);
        assert_eq!(progress.deduplicated(), 2);
    }

    /// happy path：收口写入作业且报告引用保持不变。
    #[test]
    fn apply_to_job_writes_five_counters() {
        let mut progress = BackfillProgress::new();
        progress
            .record_item(BackfillItemClassification {
                result: BackfillItemResult::New,
                cost_basis: BackfillCostBasis::Standard,
            })
            .unwrap();
        progress.record_duplicate().unwrap();

        let mut job = MallConsumptionBackfillJob::new(
            MallConsumptionBackfillJobId::new("job-1"),
            MallConsumptionBackfillJobData {
                mall_id: "mall-a".to_string(),
                cutover_id: MallConsumptionCutoverId::new("cutover-1"),
                range_start: Instant::from_unix_secs(100),
                range_end: Instant::from_unix_secs(200),
                total_count: 10,
                total_amount: Amount::from_str("100.00").unwrap(),
            },
        )
        .unwrap();
        progress.apply_to_job(&mut job).unwrap();
        assert_eq!(job.deduplicated_count, 1);
        assert_eq!(job.standard_count, 1);
        assert!(job.report_file_id.is_none());
    }

    /// 失败路径：合计超过来源总笔数时作业拒绝收口。
    #[test]
    fn apply_to_job_rejects_total_overflow() {
        let mut progress = BackfillProgress::new();
        for _ in 0..3 {
            progress
                .record_item(BackfillItemClassification {
                    result: BackfillItemResult::New,
                    cost_basis: BackfillCostBasis::Actual,
                })
                .unwrap();
        }
        let mut job = MallConsumptionBackfillJob::new(
            MallConsumptionBackfillJobId::new("job-1"),
            MallConsumptionBackfillJobData {
                mall_id: "mall-a".to_string(),
                cutover_id: MallConsumptionCutoverId::new("cutover-1"),
                range_start: Instant::from_unix_secs(100),
                range_end: Instant::from_unix_secs(200),
                total_count: 2,
                total_amount: Amount::from_str("100.00").unwrap(),
            },
        )
        .unwrap();
        assert!(progress.apply_to_job(&mut job).is_err());
    }

    /// 失败路径：计数上溢返回错误且累加器保持原值。
    #[test]
    fn record_rejects_counter_overflow() {
        let mut progress = BackfillProgress {
            deduplicated: u64::MAX,
            actual: u64::MAX,
            standard: 0,
            none: 0,
            unattributed: 0,
            failed: 0,
        };
        assert!(progress.record_duplicate().is_err());
        assert_eq!(progress.deduplicated(), u64::MAX);
        assert!(progress
            .record_item(BackfillItemClassification {
                result: BackfillItemResult::New,
                cost_basis: BackfillCostBasis::Actual,
            })
            .is_err());
        assert_eq!(progress.actual(), u64::MAX);
    }

    /// 边界：`Failed` 结果计失败口径，不计成功；成本 `None` 口径保持作业视图语义。
    #[test]
    fn failed_result_counts_as_failed_not_success() {
        let mut progress = BackfillProgress::new();
        let failed =
            BackfillItemClassification::from_mall_fact(FactType::PaymentSucceeded, ProcessingStatus::Saved);
        assert_eq!(failed.result, BackfillItemResult::Failed);
        progress.record_item(failed).unwrap();

        assert_eq!(progress.failed(), 1);
        assert_eq!(progress.succeeded(), 0);
        assert_eq!(progress.none(), 1);
        assert_eq!(progress.unattributed(), 0);
        assert_eq!(
            progress.succeeded() + progress.failed() + progress.deduplicated(),
            1,
            "成功／失败／去重之和恒等于上报总数"
        );
    }

    /// 边界：失败计数上溢返回错误且累加器保持原值。
    #[test]
    fn record_rejects_failed_counter_overflow() {
        let mut progress = BackfillProgress {
            deduplicated: 0,
            actual: 0,
            standard: 0,
            none: 0,
            unattributed: 0,
            failed: u64::MAX,
        };
        assert!(progress
            .record_item(BackfillItemClassification {
                result: BackfillItemResult::Failed,
                cost_basis: BackfillCostBasis::None,
            })
            .is_err());
        assert_eq!(progress.failed(), u64::MAX);
    }
}
