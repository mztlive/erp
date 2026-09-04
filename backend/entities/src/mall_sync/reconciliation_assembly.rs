//! 核对作业聚合装配：作业与其差异明细的一次性确定性组装。
//!
//! 计数派生（差异数量 = 明细条数）、终态登记与明细形状校验属于聚合不变式，
//! 由本域工厂独占；主键与时间由调用方注入，跨聚合存在性校验与事务仍归服务。

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{MallSalesReconciliationItemId, MallSalesReconciliationJobId};

use super::mall_sales_reconciliation::{
    MallSalesReconciliationItem, MallSalesReconciliationItemData, MallSalesReconciliationJob,
    MallSalesReconciliationJobData, ReconciliationJobStatus,
};

/// 单条差异明细的确定性种子。
#[derive(Debug, Clone)]
pub struct ReconciliationItemSeed {
    /// 明细主键（调用方生成的稳定 ID）。
    pub item_id: MallSalesReconciliationItemId,
    /// 明细创建数据。
    pub data: MallSalesReconciliationItemData,
}

/// 装配完成的核对作业聚合。
#[derive(Debug, Clone)]
pub struct AssembledReconciliation {
    /// 已登记计数并进入有差异终态的核对作业。
    pub job: MallSalesReconciliationJob,
    /// 与作业同批持久化的差异明细。
    pub items: Vec<MallSalesReconciliationItem>,
}

impl MallSalesReconciliationJob {
    /// 一次性装配核对作业与其差异明细。
    ///
    /// # 参数
    /// * `job_id` - 核对作业主键（调用方生成的稳定 ID）
    /// * `data` - 作业创建数据（来源商城、批次号、清单边界与开始时间）
    /// * `source_count` - 商城清单数量
    /// * `erp_count` - ERP 数量
    /// * `seeds` - 差异明细种子（主键与创建数据，非空）
    /// * `finished_at` - 调用方时间，登记为作业结束时间
    ///
    /// # 返回
    /// 返回计数已登记、终态为有差异的作业与全部差异明细。
    ///
    /// # 错误
    /// 明细种子为空、批次号非法、计数不可能、明细形状与 ERP 存在性
    /// 不一致，或终态与计数矛盾时返回错误。
    ///
    /// # 约束
    /// 纯聚合构造，不访问数据库、不生成 ID、不读取全局时钟；
    /// 差异数量恒等于明细条数，调用方不得另行指定。
    pub fn assemble(
        job_id: MallSalesReconciliationJobId,
        data: MallSalesReconciliationJobData,
        source_count: u64,
        erp_count: u64,
        seeds: Vec<ReconciliationItemSeed>,
        finished_at: Instant,
    ) -> Result<AssembledReconciliation> {
        if seeds.is_empty() {
            return Err(Error::from("核对差异明细不能为空"));
        }
        let mut job = Self::new(job_id, data)?;
        job.record_counts(source_count, erp_count, seeds.len() as u64)?;
        job.finish(ReconciliationJobStatus::HasDifference, finished_at)?;
        let items = seeds
            .into_iter()
            .map(|seed| MallSalesReconciliationItem::new(seed.item_id, seed.data))
            .collect::<Result<Vec<_>>>()?;
        Ok(AssembledReconciliation { job, items })
    }
}

#[cfg(test)]
mod tests {
    use super::{MallSalesReconciliationJob, ReconciliationItemSeed};
    use crate::common::time::Instant;
    use crate::ids::{
        MallSalesReconciliationItemId, MallSalesReconciliationJobId, MallSalesSyncJobId, SalesOrderId,
        SourceSystemId,
    };
    use crate::mall_sync::{
        MallSalesReconciliationItemData, MallSalesReconciliationJobData, ReconciliationDifferenceType,
        ReconciliationJobStatus,
    };

    fn job_data() -> MallSalesReconciliationJobData {
        MallSalesReconciliationJobData {
            source_system_id: SourceSystemId::new("mall-1"),
            job_no: "REC-1".to_string(),
            source_list_as_of: Instant::from_unix_secs(1_700_000_000),
            started_at: Instant::from_unix_secs(1_700_000_100),
        }
    }

    fn seed(
        index: u64,
        difference_type: ReconciliationDifferenceType,
        sales_order_id: Option<SalesOrderId>,
    ) -> ReconciliationItemSeed {
        ReconciliationItemSeed {
            item_id: MallSalesReconciliationItemId::new(format!("item-{index}")),
            data: MallSalesReconciliationItemData {
                reconciliation_job_id: MallSalesReconciliationJobId::new("job-1"),
                external_order_no: format!("SO-{index}"),
                source_status_code: "EFFECTIVE".to_string(),
                source_updated_at: Instant::from_unix_secs(1_700_000_000),
                source_content_hash: None,
                sales_order_id,
                erp_revision_id: None,
                erp_content_hash: None,
                difference_type,
            },
        }
    }

    #[test]
    fn assemble_derives_counts_and_terminal_state() {
        let finished_at = Instant::from_unix_secs(1_700_000_200);
        let assembled = MallSalesReconciliationJob::assemble(
            MallSalesReconciliationJobId::new("job-1"),
            job_data(),
            100,
            98,
            vec![
                seed(1, ReconciliationDifferenceType::ErpMissing, None),
                seed(
                    2,
                    ReconciliationDifferenceType::StatusDifference,
                    Some(SalesOrderId::new("so-2")),
                ),
            ],
            finished_at,
        )
        .unwrap();
        assert_eq!(assembled.job.job_no, "REC-1");
        assert_eq!(
            (
                assembled.job.source_count,
                assembled.job.erp_count,
                assembled.job.difference_count
            ),
            (100, 98, 2)
        );
        assert_eq!(assembled.job.status, ReconciliationJobStatus::HasDifference);
        assert_eq!(assembled.job.finished_at, Some(finished_at));
        assert_eq!(assembled.items.len(), 2);
        assert_eq!(assembled.items[0].external_order_no.as_str(), "SO-1");
    }

    #[test]
    fn assemble_rejects_empty_seeds() {
        assert!(MallSalesReconciliationJob::assemble(
            MallSalesReconciliationJobId::new("job-empty"),
            job_data(),
            100,
            100,
            Vec::new(),
            Instant::from_unix_secs(1_700_000_200),
        )
        .is_err());
    }

    #[test]
    fn assemble_rejects_impossible_counts_and_bad_item_shape() {
        assert!(MallSalesReconciliationJob::assemble(
            MallSalesReconciliationJobId::new("job-counts"),
            job_data(),
            0,
            0,
            vec![seed(1, ReconciliationDifferenceType::ErpMissing, None)],
            Instant::from_unix_secs(1_700_000_200),
        )
        .is_err());
        assert!(MallSalesReconciliationJob::assemble(
            MallSalesReconciliationJobId::new("job-shape"),
            job_data(),
            1,
            1,
            vec![seed(
                1,
                ReconciliationDifferenceType::ErpMissing,
                Some(SalesOrderId::new("so-1")),
            )],
            Instant::from_unix_secs(1_700_000_200),
        )
        .is_err());
    }

    #[test]
    fn backfill_job_reference_is_assignable_after_assemble() {
        let assembled = MallSalesReconciliationJob::assemble(
            MallSalesReconciliationJobId::new("job-bf"),
            job_data(),
            1,
            0,
            vec![seed(1, ReconciliationDifferenceType::ErpMissing, None)],
            Instant::from_unix_secs(1_700_000_200),
        )
        .unwrap();
        let mut item = assembled.items.into_iter().next().unwrap();
        item.start_backfill(MallSalesSyncJobId::new("bf-1")).unwrap();
        assert_eq!(
            item.single_order_sync_job_id,
            Some(MallSalesSyncJobId::new("bf-1"))
        );
    }
}
