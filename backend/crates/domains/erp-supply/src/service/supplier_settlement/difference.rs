use std::slice::from_ref;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::SupplierSettlementStatementId;
use erp_permissions::supplier_settlement;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::evidence::ensure_difference_version;
use super::shared::*;
use super::{SettlementAction, SupplierSettlementService};
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
    /// * `actor` - 当前认证账号。
    ///
    /// # 返回
    /// 功能资格、分页和计数共用同一快照；提交成功后返回分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn supplier_settlement_difference_list(
        &self,
        params: &SupplierSettlementDifferenceListParams,
        actor: &AuditActor,
    ) -> Result<SettlementPageView<SupplierSettlementDifferenceView>> {
        let service = Self::new(self.db.clone()).with_data_scope(self.access());
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.difference_page(&params, &actor, executor).await })
            })
            .await
    }

    /// 在原读取快照检查功能资格，并使用原执行器读取分页和计数。
    /// # 参数
    /// params 为列表条件，actor 为认证账号，executor 为公开入口的原事务执行器。
    /// # 返回
    /// 功能资格检查通过后的结算差异分页。
    /// # 错误
    /// 授权、参数校验或业务读取失败时返回对应错误。
    pub(super) async fn difference_page(
        &self,
        params: &SupplierSettlementDifferenceListParams,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SettlementPageView<SupplierSettlementDifferenceView>> {
        self.access().require_operation(actor, &supplier_settlement::DIFFERENCE_LIST, executor).await?;
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
            .search_supplier_settlement_differences(&filter, executor)
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

    /// 先授权结算单，再读取差异、核验归属和版本并准备正式结论。
    ///
    /// # 参数
    /// * `id` - 结算差异主键。
    /// * `req` - 差异决定请求，含结算单、期望版本和操作身份。
    /// * `conclusion` - 已规范化的正式结论。
    /// * `actor` - 当前真实差异处理账号。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回已更新主题摘要的结算单，以及已记入结论但尚未写回的差异。
    ///
    /// # 错误
    /// 差异或所属明细不存在时返回 `NotFound`。版本不一致、差异不属于当前结算单时返回 `ConflictError`。操作人不是差异处理人时返回 `Forbidden`。结算单不可编辑，或结论引用了未登记证据时返回 `BusinessLogicError`。结论记录或仓储读取失败时返回对应错误。
    pub async fn prepare_difference_decision(
        &self,
        id: &str,
        req: &SettlementDifferenceDecisionRequest,
        conclusion: &SettlementDifferenceConclusion,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(SupplierSettlementStatement, SupplierSettlementDifference)> {
        self.access()
            .require_action(actor, SettlementAction::ResolveDifference, &req.statement_id, executor)
            .await?;
        let mut difference = self.load_decision_difference(id, req, executor).await?;
        let mut statement = self.load_statement(&req.statement_id, executor).await?;
        statement
            .ensure_version(req.expected_lock_version)
            .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
        if !statement.is_editable() {
            return Err(Error::BusinessLogicError("当前结算状态禁止处理差异".to_string()));
        }
        self.ensure_conclusion_evidence(&difference, conclusion, executor).await?;
        let now = Instant::now();
        difference.record_conclusion(conclusion, actor.id(), now)?;
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

    /// 结算单授权后核验差异版本和明细归属。
    async fn load_decision_difference(
        &self,
        id: &str,
        req: &SettlementDifferenceDecisionRequest,
        executor: &mut dyn Executor,
    ) -> Result<SupplierSettlementDifference> {
        let difference = self
            .db
            .supplier_settlement_differences()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("结算差异不存在".to_string()))?;
        ensure_difference_version(difference.base.version, req.expected_difference_version)?;
        let item = self
            .db
            .supplier_settlement_items()
            .find_by_id(&difference.statement_item_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("结算差异所属明细不存在".to_string()))?;
        let statement_id = SupplierSettlementStatementId::new(&req.statement_id);
        if !item.belongs_to_statement(&statement_id) {
            return Err(Error::ConflictError("结算差异已不属于当前结算单".to_string()));
        }
        Ok(difference)
    }
    /// 确保正式差异结论仅引用该差异已登记的证据。
    async fn ensure_conclusion_evidence(
        &self,
        difference: &SupplierSettlementDifference,
        conclusion: &SettlementDifferenceConclusion,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if !conclusion.evidence_reference_ids().is_empty() {
            let stored_evidence = self
                .db
                .supplier_settlement_difference_evidence()
                .find_by_difference_ids(from_ref(&difference.base.id), executor)
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
        Ok(())
    }

    /// 同一执行器依次 CAS 结算单和差异，审计仅在两步成功后写入。
    ///
    /// # 参数
    /// * `statement` - 待按 CAS 写回的结算单。
    /// * `difference` - 待按 CAS 写回的差异。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 无返回值。先写结算单，再写差异。
    ///
    /// # 错误
    /// 任一步仓储更新失败时返回对应错误。
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
///
/// # 错误
/// 不返回错误。
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
///
/// # 错误
/// 不返回错误。
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
/// 返回不含补证列表的差异响应视图。
///
/// # 错误
/// 不返回错误。
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

#[cfg(test)]
mod authorization_tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::service::supplier_settlement::SettlementAccess;

    #[tokio::test]
    async fn settlement_preparation_stops_before_business_reads_when_authorization_fails() {
        let client = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let db = client.database("settlement_preparation_authorization_unit");
        let access = SettlementAccess::new(db.clone(), unavailable_authorization());
        let service = SupplierSettlementService::new(db).with_data_scope(access);
        let actor = AuditActor::new("actor".into(), "operator".into(), AccountKind::Admin);
        let refresh = RefreshSettlementStatementRequest {
            action: SettlementDraftAction::Refresh,
            statement_id: "statement".into(),
            expected_lock_version: 7,
            expected_source_snapshot_hash: "a".repeat(64),
            request_id: "refresh-operation".into(),
            idempotency_key: "refresh-key".into(),
        };
        let decision = SettlementDifferenceDecisionRequest {
            statement_id: "statement".into(),
            difference_id: "difference".into(),
            expected_lock_version: 7,
            expected_difference_version: 8,
            resolution: SettlementDifferenceResolution::SupplierAccepted,
            reason_code: "BILL_ALIGNED".into(),
            evidence_reference_ids: Vec::new(),
            operation_id: "difference-operation".into(),
            idempotency_key: "difference-key".into(),
        };
        let conclusion = SettlementDifferenceConclusion::new(
            SettlementDifferenceConclusionKind::SupplierAccepted,
            &decision.reason_code,
            Vec::new(),
        )
        .unwrap();
        let results = [
            service
                .prepare_refresh("statement", &refresh, &actor, &mut SnapshotExecutor::default())
                .await
                .map(|_| ()),
            service
                .prepare_difference_decision(
                    "difference",
                    &decision,
                    &conclusion,
                    &actor,
                    &mut SnapshotExecutor::default(),
                )
                .await
                .map(|_| ()),
        ];
        for result in results {
            assert!(matches!(result, Err(Error::Internal(reason)) if reason == "测试授权设施不可用"));
        }
    }
}
