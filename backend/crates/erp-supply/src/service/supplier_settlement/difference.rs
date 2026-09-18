use erp_core::common::time::Instant;
use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use super::SupplierSettlementService;
use super::shared::*;
use crate::dto::supplier_fulfillment::SortDir;
use crate::dto::supplier_settlement as dto;
use crate::dto::supplier_settlement::*;
use crate::entity::supplier_settlement::*;
use crate::repository::SupplierSettlementExt;
use crate::repository::prelude::*;
use crate::{Error, Result};
/// 结算差异列表筛选条件类型。
type DifferenceFilter = <mongodb::Database as SupplierSettlementExt>::SupplierSettlementDifferenceFilter;
impl SupplierSettlementService {
    /// 分页查询供应商结算差异列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`statement_item_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn supplier_settlement_difference_list(
        &self,
        params: &SupplierSettlementDifferenceListParams,
    ) -> Result<SettlementPageView<SupplierSettlementDifferenceView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = DifferenceFilter {
            statement_item_id: query.statement_item_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .supplier_settlement_differences()
            .search_supplier_settlement_differences(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SupplierSettlementDifferenceView {
                id: row.id,
                statement_item_id: row.statement_item_id.to_string(),
                difference_type: row.difference_type,
                difference_amount: row.difference_amount,
                status: row.status,
                resolution: row.resolution,
                resolved_by: row.resolved_by,
                resolved_at: row.resolved_at.map(|t| t.unix_secs()),
                version: row.version,
                created_at: row.created_at,
                evidence: Vec::new(),
            })
            .collect();

        Ok(SettlementPageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
    }

    /// 结算本域prepare_difference_decision，保持原校验、构造和执行器顺序。
    pub async fn prepare_difference_decision(
        &self,
        id: &str,
        req: &SettlementDifferenceDecisionRequest,
        conclusion: &SettlementDifferenceConclusion,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<(SupplierSettlementStatement, SupplierSettlementDifference)> {
        let mut difference = self
            .db
            .supplier_settlement_differences()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("结算差异不存在".to_string()))?;
        super::evidence::ensure_difference_version(difference.base.version, req.expected_difference_version)?;
        let item = self
            .db
            .supplier_settlement_items()
            .find_by_id(&difference.statement_item_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("结算差异所属明细不存在".to_string()))?;
        let statement_id = erp_core::ids::SupplierSettlementStatementId::new(&req.statement_id);
        if !item.belongs_to_statement(&statement_id) {
            return Err(Error::ConflictError("结算差异已不属于当前结算单".to_string()));
        }
        let mut statement = self.load_statement(&req.statement_id, executor).await?;
        statement
            .ensure_version(req.expected_lock_version)
            .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
        if !statement.is_difference_handler(actor_id) {
            return Err(Error::Forbidden("只有当前差异处理人可以登记正式差异结论".to_string()));
        }
        if !statement.is_editable() {
            return Err(Error::BusinessLogicError("当前结算状态禁止处理差异".to_string()));
        }
        if !conclusion.evidence_reference_ids().is_empty() {
            let stored_evidence = self
                .db
                .supplier_settlement_difference_evidence()
                .find_by_difference_ids(&[difference.base.id.clone()], executor)
                .await?;
            let stored_references = stored_evidence
                .iter()
                .flat_map(|evidence| evidence.evidence_reference_ids.iter())
                .map(String::as_str)
                .collect::<std::collections::HashSet<_>>();
            if conclusion
                .evidence_reference_ids()
                .iter()
                .any(|reference| !stored_references.contains(reference.as_str()))
            {
                return Err(Error::BusinessLogicError(
                    "差异决定引用了尚未通过补证命令登记的证据".to_string(),
                ));
            }
        }
        let now = Instant::now();
        difference.record_conclusion(conclusion, actor_id, now)?;
        let items = self.load_statement_items(&statement.base.id, executor).await?;
        let mut differences = self.load_statement_differences(&items, executor).await?;
        let stored = differences
            .iter_mut()
            .find(|stored| stored.base.id == difference.base.id)
            .ok_or_else(|| Error::ConflictError("结算差异已不属于当前结算单".to_string()))?;
        *stored = difference.clone();
        statement.update(SupplierSettlementStatementUpdate {
            status: Some(SettlementStatus::HasDifference),
            ..Default::default()
        })?;
        statement.update_subject_hash(statement.review_subject_hash(&differences))?;

        Ok((statement, difference))
    }
    /// 同一执行器依次 CAS 结算单和差异，审计仅在两步成功后写入。
    pub async fn persist_difference_decision(
        &self,
        statement: &mut SupplierSettlementStatement,
        difference: &mut SupplierSettlementDifference,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.supplier_settlement_statements().update(statement, executor).await?;
        self.db.supplier_settlement_differences().update(difference, executor).await?;
        Ok(())
    }
}
/// 将 API 差异决定适配为领域结论类别。
///
/// # 参数
/// * `resolution` - 客户端提交的强类型差异决定
///
/// # 返回
/// 返回实体层用于校验原因、证据和状态推进的结论类别。
pub fn difference_conclusion_kind(
    resolution: dto::SettlementDifferenceResolution,
) -> SettlementDifferenceConclusionKind {
    match resolution {
        dto::SettlementDifferenceResolution::SupplierAccepted => {
            SettlementDifferenceConclusionKind::SupplierAccepted
        },
        dto::SettlementDifferenceResolution::ErpAccepted => SettlementDifferenceConclusionKind::ErpAccepted,
        dto::SettlementDifferenceResolution::Compensated => SettlementDifferenceConclusionKind::Compensated,
        dto::SettlementDifferenceResolution::ClosedNoAdjustment => {
            SettlementDifferenceConclusionKind::ClosedNoAdjustment
        },
    }
}

/// 计算差异决定命令指纹。
///
/// # 参数
/// * `req` - 原始差异决定请求
/// * `conclusion` - 已由实体值对象规范化的正式结论
///
/// # 返回
/// 返回不受证据输入顺序和重复项影响的稳定 SHA-256 指纹。
pub fn difference_decision_fingerprint(
    req: &SettlementDifferenceDecisionRequest,
    conclusion: &SettlementDifferenceConclusion,
) -> String {
    digest_parts(&[
        req.statement_id.clone(),
        req.difference_id.clone(),
        req.expected_lock_version.to_string(),
        req.expected_difference_version.to_string(),
        conclusion.kind().as_str().to_string(),
        conclusion.reason_code().to_string(),
        conclusion.evidence_reference_ids().join(","),
        req.operation_id.clone(),
    ])
}

/// 从结算差异实体构造响应视图。
///
/// # 参数
/// * `difference` - 结算差异实体
///
/// # 返回
/// 返回响应视图。
pub fn settlement_difference_view(
    difference: SupplierSettlementDifference,
) -> SupplierSettlementDifferenceView {
    SupplierSettlementDifferenceView {
        id: difference.base.id,
        statement_item_id: difference.statement_item_id.to_string(),
        difference_type: difference.difference_type,
        difference_amount: difference.difference_amount,
        status: difference.status,
        resolution: difference.resolution,
        resolved_by: difference.resolved_by,
        resolved_at: difference.resolved_at.map(|t| t.unix_secs()),
        version: difference.base.version,
        created_at: difference.base.created_at,
        evidence: Vec::new(),
    }
}
