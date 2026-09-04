//! 回填提交规划（INT-R15 分类与明细装配 ＋ INT-E08 进度折叠）。
//!
//! 原 `submit_backfill_command` 在事务内逐项查重、逐条创建并手工维护五类计数器；
//! 现调用方先由 Repository 一次批量取回已存在业务键，再调本文件的纯装配规划产出
//! 新增明细与进度累加器，最后由 Repository 一次批量追加。事务、幂等、版本守卫与
//! 最终唯一索引约束仍由 `mod.rs` 持有。

use std::collections::HashSet;

use entities::ids::{MallConsumptionBackfillItemId, MallConsumptionBackfillJobId};
use entities::mall_backfill::{
    BackfillItemClassification, BackfillProgress, MallConsumptionBackfillItem,
    MallConsumptionBackfillItemData,
};
use entities::mall_order::MallOrderFact;
use id_generator::next_id;

use crate::errors::Result;

/// 按稳定的事实顺序规划新增明细并折叠进度（INT-R15 ＋ INT-E08）。
///
/// 已存在键（库内批量预查命中）与本请求内重复键均计为去重，不产生明细；其余事实
/// 按 `BackfillItemClassification::from_mall_fact` 分类装配明细并同步推进进度。
/// 调用方保证输入顺序即持久化顺序（`occurred_at, id` 升序）；明细 ID 与时间由
/// Service 侧生成并注入，领域层只做分类与计数。
///
/// # 参数
/// * `job_id` - 回填批次 ID（装配明细的归属批次）
/// * `facts` - 范围内关键事实（已按发生时间与事实 ID 稳定排序）
/// * `existing_keys` - 同批次已存在的业务键集合（批量预查结果）
///
/// # 返回
/// 返回按输入顺序排列的新增明细与同步折叠的进度累加器。
///
/// # 错误
/// 空业务键、明细构造校验失败或任一进度计数上溢时返回错误，且不返回部分装配结果。
///
/// # 约束
/// 本函数不访问数据库与外部 I/O；最终唯一性仍由唯一索引保证，`DuplicateKey`
/// 由调用方事务整体回滚，不在此消化。
pub(crate) fn plan_backfill_items(
    job_id: &MallConsumptionBackfillJobId,
    facts: &[MallOrderFact],
    existing_keys: &HashSet<String>,
) -> Result<(Vec<MallConsumptionBackfillItem>, BackfillProgress)> {
    let mut progress = BackfillProgress::new();
    let mut seen: HashSet<&str> = HashSet::with_capacity(facts.len());
    let mut items = Vec::new();
    for fact in facts {
        ensure_business_fact_key(&fact.business_fact_key)?;
        if existing_keys.contains(&fact.business_fact_key) || !seen.insert(fact.business_fact_key.as_str()) {
            progress.record_duplicate()?;
            continue;
        }
        let classification =
            BackfillItemClassification::from_mall_fact(fact.fact_type, fact.processing_status);
        items.push(new_backfill_item(job_id, fact, &classification)?);
        progress.record_item(classification)?;
    }
    Ok((items, progress))
}

/// 显式拒绝空业务键（fail-closed，与批量预查一致）。
///
/// # 参数
/// * `key` - 待装配事实的业务键
///
/// # 返回
/// 非空时返回 `Ok(())`。
///
/// # 错误
/// 业务键为空时返回错误；调用方不得产生部分装配结果。
///
/// # 约束
/// 纯输入守卫；实体构造侧同样拒绝空键，双层一致。
fn ensure_business_fact_key(key: &str) -> Result<()> {
    if key.is_empty() {
        return Err(crate::errors::Error::BusinessLogicError(
            "业务事实键不能为空".to_string(),
        ));
    }
    Ok(())
}

/// 由单条关键事实装配回填明细实体。
///
/// # 参数
/// * `job_id` - 回填批次 ID
/// * `fact` - 待回填的关键事实
/// * `classification` - 已派生的明细分类
///
/// # 返回
/// 返回新建的回填明细实体。
///
/// # 错误
/// 明细结果一致性校验失败时返回错误。
///
/// # 约束
/// 明细 ID 由 Service 侧生成；错误码／详情恒为空（失败形态不在回填提交内产生）。
fn new_backfill_item(
    job_id: &MallConsumptionBackfillJobId,
    fact: &MallOrderFact,
    classification: &BackfillItemClassification,
) -> Result<MallConsumptionBackfillItem> {
    Ok(MallConsumptionBackfillItem::new(
        MallConsumptionBackfillItemId::new(next_id()),
        MallConsumptionBackfillItemData {
            job_id: job_id.clone(),
            business_fact_key: fact.business_fact_key.clone(),
            source_event_reference: fact.source_event_id.clone(),
            inbox_message_id: fact.inbox_message_id.clone(),
            mall_order_fact_id: Some(fact.base.id.clone().into()),
            result: classification.result,
            cost_basis: classification.cost_basis,
            error_code: None,
            error_detail: None,
        },
    )?)
}

#[cfg(test)]
mod tests {
    use super::{ensure_business_fact_key, plan_backfill_items};
    use entities::common::time::Instant;
    use entities::ids::{InboxMessageId, MallConsumptionBackfillJobId, MallOrderFactId};
    use entities::mall_order::{DataSource, FactType, MallOrderFact, MallOrderFactData, ProcessingStatus};
    use std::collections::HashSet;

    /// 构造指定业务键与处理状态的关键事实。
    fn fact_with_key(key: &str, status: ProcessingStatus) -> MallOrderFact {
        let mut fact = MallOrderFact::new(
            MallOrderFactId::new(format!("fact-{key}")),
            MallOrderFactData {
                mall_id: "mall-a".to_string(),
                source_event_id: format!("evt-{key}"),
                inbox_message_id: InboxMessageId::new(format!("inbox-{key}")),
                fact_type: FactType::PaymentSucceeded,
                business_fact_key: key.to_string(),
                external_order_no: "SO-1".to_string(),
                external_order_version: "v1".to_string(),
                after_sales_request_id: None,
                original_payment_fact_id: None,
                occurred_at: Instant::from_unix_secs(1_700_000_000),
                received_at: Instant::from_unix_secs(1_700_000_100),
                data_source: DataSource::Realtime,
                raw_payload_reference: None,
            },
        )
        .unwrap();
        if status == ProcessingStatus::PendingAttribution {
            fact.update_processing_status(ProcessingStatus::PendingAttribution)
                .unwrap();
        } else if status == ProcessingStatus::Attributed {
            fact.update_processing_status(ProcessingStatus::PendingAttribution)
                .unwrap();
            fact.update_processing_status(ProcessingStatus::Attributed)
                .unwrap();
        } else if status == ProcessingStatus::Difference {
            fact.update_processing_status(ProcessingStatus::Difference)
                .unwrap();
        } else if status == ProcessingStatus::Rejected {
            fact.update_processing_status(ProcessingStatus::Rejected).unwrap();
        }
        fact
    }

    /// happy path：空输入产出空明细与零进度。
    #[test]
    fn plan_empty_facts_yields_empty_items_and_zero_progress() {
        let job_id = MallConsumptionBackfillJobId::new("job-1");
        let (items, progress) = plan_backfill_items(&job_id, &[], &HashSet::new()).unwrap();
        assert!(items.is_empty());
        assert_eq!(progress.succeeded(), 0);
        assert_eq!(progress.deduplicated(), 0);
    }

    /// happy path：库内重复计去重且不产生明细。
    #[test]
    fn plan_counts_existing_keys_as_duplicates() {
        let job_id = MallConsumptionBackfillJobId::new("job-1");
        let facts = vec![
            fact_with_key("k1", ProcessingStatus::Attributed),
            fact_with_key("k2", ProcessingStatus::Attributed),
        ];
        let existing: HashSet<String> = HashSet::from(["k1".to_string()]);
        let (items, progress) = plan_backfill_items(&job_id, &facts, &existing).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].business_fact_key, "k2");
        assert_eq!(progress.deduplicated(), 1);
        assert_eq!(progress.succeeded(), 1);
        assert_eq!(progress.actual(), 1);
    }

    /// 边界：请求内重复键计去重，首项顺序即写入顺序。
    #[test]
    fn plan_counts_in_request_duplicates_without_extra_writes() {
        let job_id = MallConsumptionBackfillJobId::new("job-1");
        let facts = vec![
            fact_with_key("k1", ProcessingStatus::Attributed),
            fact_with_key("k1", ProcessingStatus::Attributed),
            fact_with_key("k2", ProcessingStatus::Attributed),
        ];
        let (items, progress) = plan_backfill_items(&job_id, &facts, &HashSet::new()).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(progress.deduplicated(), 1);
        assert_eq!(progress.succeeded(), 2);
    }

    /// happy path：待归集事实同步计未归集口径。
    #[test]
    fn plan_folds_pending_attribution_counts() {
        let job_id = MallConsumptionBackfillJobId::new("job-1");
        let facts = vec![fact_with_key("k1", ProcessingStatus::PendingAttribution)];
        let (items, progress) = plan_backfill_items(&job_id, &facts, &HashSet::new()).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(progress.unattributed(), 1);
        assert_eq!(progress.none(), 1);
    }

    /// 边界：大批量下成功与去重之和恒等于输入总数。
    #[test]
    fn plan_large_batch_keeps_conservation() {
        let job_id = MallConsumptionBackfillJobId::new("job-1");
        let facts: Vec<MallOrderFact> = (0..200)
            .map(|index| fact_with_key(&format!("k{index}"), ProcessingStatus::Attributed))
            .collect();
        let existing: HashSet<String> = (0..50).map(|index| format!("k{index}")).collect();
        let (items, progress) = plan_backfill_items(&job_id, &facts, &existing).unwrap();
        assert_eq!(items.len(), 150);
        assert_eq!(progress.deduplicated(), 50);
        assert_eq!(progress.failed(), 0);
        assert_eq!(
            progress.succeeded() + progress.failed() + progress.deduplicated(),
            200
        );
    }

    /// 失败路径：`Saved` 事实分类为失败，明细构造首错失败关闭且无部分结果。
    #[test]
    fn plan_saved_fact_fails_closed_without_partial_items() {
        let job_id = MallConsumptionBackfillJobId::new("job-1");
        let facts = vec![
            fact_with_key("k-ok", ProcessingStatus::Attributed),
            fact_with_key("k-saved", ProcessingStatus::Saved),
        ];
        let err = plan_backfill_items(&job_id, &facts, &HashSet::new()).expect_err("失败分类必须首错返回");
        assert!(
            err.to_string().contains("失败明细必须携带错误码"),
            "首错必须指向失败明细约束：{err}"
        );
    }

    /// 失败路径：`Rejected` 事实同样首错失败关闭。
    #[test]
    fn plan_rejected_fact_fails_closed() {
        let job_id = MallConsumptionBackfillJobId::new("job-1");
        let facts = vec![fact_with_key("k-rejected", ProcessingStatus::Rejected)];
        assert!(plan_backfill_items(&job_id, &facts, &HashSet::new()).is_err());
    }

    /// happy path：`Difference` 事实按待归集折叠未归集口径。
    #[test]
    fn plan_difference_fact_folds_pending_attribution() {
        let job_id = MallConsumptionBackfillJobId::new("job-1");
        let facts = vec![fact_with_key("k-diff", ProcessingStatus::Difference)];
        let (items, progress) = plan_backfill_items(&job_id, &facts, &HashSet::new()).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(progress.unattributed(), 1);
        assert_eq!(progress.failed(), 0);
        assert_eq!(progress.succeeded(), 1);
    }

    /// 边界：空业务键守卫显式拒绝（与批量预查一致的 fail-closed）。
    #[test]
    fn empty_business_fact_key_is_rejected() {
        assert!(ensure_business_fact_key("k1").is_ok());
        assert!(ensure_business_fact_key("").is_err());
    }
}
