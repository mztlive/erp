//! 供应商结算单详情快照（FUL-R07）。
//!
//! 把结算详情固定的四段持久化关联（结算头、明细、差异、差异补证）收敛为一次
//! 有界批量读取，替代原来散落在 Service 的关系加载与归组。

use crate::repository::owned::{
    SupplierSettlementDifferenceEvidenceRepository, SupplierSettlementDifferenceRepository,
    SupplierSettlementItemRepository, SupplierSettlementStatementRepository,
};
use std::collections::BTreeMap;

use crate::entity::supplier_settlement::{
    SupplierSettlementDifference, SupplierSettlementDifferenceEvidence, SupplierSettlementItem,
    SupplierSettlementStatement,
};
use mongodb::bson::doc;

use super::super::extensions::SupplierSettlementExt;
use super::SupplierSettlementRepository;
use persistence_core::Executor;
use persistence_core::Result;

/// 供应商结算单详情的最小事实快照。
///
/// 只携带原始实体与按差异归组的补证，不做任何 View 映射、权限或跨聚合决定；
/// 调用方 Service 继续拥有 RBAC、allowed actions、成本调整决定与最终投影。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplierSettlementStatementDetailSnapshot {
    /// 未删除的结算单头。
    pub statement: SupplierSettlementStatement,
    /// 该结算单的全部未删除冻结明细，按 `created_at` 与主键稳定排序。
    pub items: Vec<SupplierSettlementItem>,
    /// 明细关联的全部未删除差异，按 `created_at` 与主键稳定排序。
    pub differences: Vec<SupplierSettlementDifference>,
    /// 按差异主键归组的原始补证实体；无补证的差异不在映射中出现。
    ///
    /// 组内保持 `provided_at` 与主键稳定顺序；只包含本次快照差异的补证，
    /// 孤立或其他结算单的补证不会泄漏进来。
    pub evidence_by_difference: BTreeMap<String, Vec<SupplierSettlementDifferenceEvidence>>,
}

impl<'a> SupplierSettlementRepository<'a> {
    /// 读取结算单详情的最小事实快照。
    ///
    /// 把详情固定的四段关联收敛为四次有界读取：结算头主键读取、明细按结算单
    /// 读取、差异按明细主键 `$in` 读取、补证按结算单与差异主键 `$in` 读取。
    /// 空明细或空差异时不再发起空 `$in` 查询，直接返回空集合。补证同时按
    /// 结算单过滤，孤立或其他结算单的补证不得泄漏进归组。
    ///
    /// # 参数
    /// * `statement_id` - 供应商结算单主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 结算单不存在或已软删除时返回 `None`（调用方映射为 `NotFound`）；
    /// 存在时返回头、明细、差异及按差异归组的原始补证；无明细、无差异、
    /// 无补证均稳定返回空集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误；金额或数量精度错误由实体
    /// 反序列化直接透出，不做静默降级。
    ///
    /// # 约束
    /// 只做事实读取与确定性归组，不开事务、不做跨聚合决定、不返回 services
    /// View；软删除过滤由基类自动追加，排序与旧 Service 路径保持一致。
    pub async fn statement_detail_snapshot(
        &self,
        statement_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierSettlementStatementDetailSnapshot>> {
        let statement = SupplierSettlementStatementRepository::new(
            self.db,
            <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_STATEMENTS,
        )
        .find_by_id(statement_id, executor)
        .await?;
        let Some(statement) = statement else {
            return Ok(None);
        };
        let items: Vec<SupplierSettlementItem> = SupplierSettlementItemRepository::new(
            self.db,
            <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_ITEMS,
        )
        .find_many_sorted(
            doc! { "statement_id": statement_id },
            doc! { "created_at": 1, "id": 1 },
            executor,
        )
        .await?;
        let item_ids = items.iter().map(|item| item.base.id.clone()).collect::<Vec<_>>();
        let differences: Vec<SupplierSettlementDifference> = if item_ids.is_empty() {
            Vec::new()
        } else {
            SupplierSettlementDifferenceRepository::new(
                self.db,
                <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_DIFFERENCES,
            )
            .find_many_sorted(
                doc! { "statement_item_id": { "$in": item_ids } },
                doc! { "created_at": 1, "id": 1 },
                executor,
            )
            .await?
        };
        let difference_ids = differences
            .iter()
            .map(|difference| difference.base.id.clone())
            .collect::<Vec<_>>();
        let evidence: Vec<SupplierSettlementDifferenceEvidence> = if difference_ids.is_empty() {
            Vec::new()
        } else {
            SupplierSettlementDifferenceEvidenceRepository::new(
                self.db,
                <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_DIFFERENCE_EVIDENCE,
            )
            .find_many_sorted(
                doc! {
                    "statement_id": statement_id,
                    "difference_id": { "$in": difference_ids },
                },
                doc! { "provided_at": 1, "id": 1 },
                executor,
            )
            .await?
        };
        Ok(Some(SupplierSettlementStatementDetailSnapshot {
            statement,
            items,
            differences,
            evidence_by_difference: group_evidence_by_difference(evidence),
        }))
    }
}

/// 按差异主键归组原始补证实体。
///
/// 输入必须已按 `provided_at` 与主键稳定排序；本函数只做确定性归组并保持
/// 每组内相对顺序，不做任何过滤或兜底替换。
///
/// # 参数
/// * `evidence` - 已按稳定顺序读取的原始补证实体
///
/// # 返回
/// 返回以差异主键为键的归组映射；空输入返回空映射。
///
/// # 错误
/// 本函数不失败；调用方查询保证只传入本次快照差异的补证。
///
/// # 约束
/// 纯内存归组，无 I/O、无时钟、无密钥；不返回 services View。
fn group_evidence_by_difference(
    evidence: Vec<SupplierSettlementDifferenceEvidence>,
) -> BTreeMap<String, Vec<SupplierSettlementDifferenceEvidence>> {
    let mut grouped: BTreeMap<String, Vec<SupplierSettlementDifferenceEvidence>> = BTreeMap::new();
    for value in evidence {
        grouped
            .entry(value.difference_id.to_string())
            .or_default()
            .push(value);
    }
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::supplier_settlement::SupplierSettlementDifferenceEvidenceData;
    use erp_core::common::time::Instant;
    use erp_core::ids::{SupplierSettlementDifferenceId, SupplierSettlementStatementId};

    /// 构造单条补证实体。
    ///
    /// # 参数
    /// * `statement_id` - 所属结算单主键
    /// * `id` - 补证主键
    /// * `difference_id` - 所属差异主键
    /// * `provided_at_secs` - 补证时间（秒级时间戳）
    ///
    /// # 返回
    /// 返回可直接归组的不可变补证实体。
    fn sample_evidence_in_statement(
        statement_id: &str,
        id: &str,
        difference_id: &str,
        provided_at_secs: i64,
    ) -> SupplierSettlementDifferenceEvidence {
        SupplierSettlementDifferenceEvidence::new(
            id,
            SupplierSettlementDifferenceEvidenceData {
                request_id: format!("request-{id}"),
                statement_id: SupplierSettlementStatementId::new(statement_id),
                difference_id: SupplierSettlementDifferenceId::new(difference_id),
                evidence_reference_ids: vec![format!("ticket://{id}")],
                opinion_code: None,
                comment: None,
                provided_by: "preparer-1".to_string(),
                provided_at: Instant::from_unix_secs(provided_at_secs),
                command_hash: "a".repeat(64),
            },
        )
        .unwrap()
    }
    /// 构造单条补证实体（归属 `statement-1`）。
    ///
    /// # 参数
    /// * `id` - 补证主键
    /// * `difference_id` - 所属差异主键
    /// * `provided_at_secs` - 补证时间（秒级时间戳）
    ///
    /// # 返回
    /// 返回可直接归组的不可变补证实体。
    fn sample_evidence(
        id: &str,
        difference_id: &str,
        provided_at_secs: i64,
    ) -> SupplierSettlementDifferenceEvidence {
        sample_evidence_in_statement("statement-1", id, difference_id, provided_at_secs)
    }
    #[test]
    fn detail_snapshot_groups_empty_evidence_as_empty_map() {
        let grouped = group_evidence_by_difference(Vec::new());
        assert!(grouped.is_empty());
    }
    #[test]
    fn detail_snapshot_groups_evidence_by_difference_without_leak() {
        let grouped = group_evidence_by_difference(vec![
            sample_evidence("evidence-1", "difference-1", 100),
            sample_evidence("evidence-2", "difference-2", 101),
            sample_evidence("evidence-3", "difference-1", 102),
        ]);
        assert_eq!(grouped.len(), 2);
        let first = grouped.get("difference-1").unwrap();
        assert_eq!(first.len(), 2);
        assert_eq!(first[0].base.id, "evidence-1");
        assert_eq!(first[1].base.id, "evidence-3");
        let second = grouped.get("difference-2").unwrap();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].base.id, "evidence-2");
    }
    #[test]
    fn detail_snapshot_keeps_stable_order_within_group() {
        let grouped = group_evidence_by_difference(vec![
            sample_evidence("evidence-b", "difference-1", 200),
            sample_evidence("evidence-a", "difference-1", 100),
        ]);
        let group = grouped.get("difference-1").unwrap();
        assert_eq!(group[0].base.id, "evidence-b");
        assert_eq!(group[1].base.id, "evidence-a");
    }
}
