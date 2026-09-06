use database::SupplierSettlementExt;
use entities::supplier_settlement::{
    SettlementDifferenceConclusion, SettlementDifferenceConclusionKind, SettlementStatus,
    SupplierSettlementDifference, SupplierSettlementStatement, SupplierSettlementStatementUpdate,
};
use erp_audit::AuditExt;
use erp_core::common::time::Instant;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::dto::{
    SettlementDifferenceDecisionRequest, SettlementDifferenceDecisionResult, SettlementPageView,
    SupplierSettlementDifferenceListParams, SupplierSettlementDifferenceView,
};
use super::{
    command_audit_id, digest_parts, dto, ensure_audit_resource, ensure_same_id, parse_receipt_number,
    receipt_result, SupplierSettlementService, COMMAND_FINGERPRINT_PREFIX,
};
use crate::errors::{Error, Result};
use crate::supplier_fulfillment::dto::SortDir;
use application_core::AuditActor;
use erp_audit::AuditActorLogs;

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

        Ok(SettlementPageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 登记财务经办的强类型差异结论。
    ///
    /// 命令同时 CAS 结算单与差异版本，规范化受控原因/证据，推进主题摘要并写
    /// 幂等审计。客户端不能提交处理人、处理时间或任意持久化状态。
    ///
    /// # 错误
    /// 路径身份、归属、版本、经办责任或证据规则不一致时 fail-closed。
    pub async fn decide_difference(
        &self,
        id: &str,
        req: SettlementDifferenceDecisionRequest,
        actor: &AuditActor,
    ) -> Result<SettlementDifferenceDecisionResult> {
        req.validate()?;
        ensure_same_id(id, &req.difference_id, "结算差异")?;
        let conclusion = SettlementDifferenceConclusion::new(
            difference_conclusion_kind(req.resolution),
            req.reason_code.clone(),
            req.evidence_reference_ids.clone(),
        )
        .map_err(|error| Error::ValidationError(error.to_string()))?;
        let fingerprint = difference_decision_fingerprint(&req, &conclusion);
        let audit_id = command_audit_id(
            actor.id(),
            "supplier_settlement.difference_decision",
            id,
            &req.idempotency_key,
        );
        if let Some(result) = self
            .replay_difference_decision(&audit_id, &fingerprint, id)
            .await?
        {
            return Ok(result);
        }
        let mut difference = self
            .db
            .supplier_settlement_differences()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("结算差异不存在".to_string()))?;
        difference
            .ensure_version(req.expected_difference_version)
            .map_err(|_| Error::ConflictError("结算差异版本已变化，请刷新后重试".to_string()))?;
        let item = self
            .db
            .supplier_settlement_items()
            .find_by_id(&difference.statement_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("结算差异所属明细不存在".to_string()))?;
        let statement_id = erp_core::ids::SupplierSettlementStatementId::new(&req.statement_id);
        if !item.belongs_to_statement(&statement_id) {
            return Err(Error::ConflictError("结算差异已不属于当前结算单".to_string()));
        }
        let mut statement = self.load_statement(&req.statement_id).await?;
        statement
            .ensure_version(req.expected_lock_version)
            .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
        if !statement.is_prepared_by(actor.id()) {
            return Err(Error::Forbidden(
                "只有当前结算经办人可以登记正式差异结论".to_string(),
            ));
        }
        if !statement.is_editable() {
            return Err(Error::BusinessLogicError("当前结算状态禁止处理差异".to_string()));
        }
        if !conclusion.evidence_reference_ids().is_empty() {
            let stored_evidence = self
                .db
                .supplier_settlement_difference_evidence()
                .find_by_difference_ids(&[difference.base.id.clone()], &mut NoTransaction)
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
        difference.record_conclusion(&conclusion, actor.id(), now)?;
        let items = self
            .load_statement_items(&statement.base.id, &mut NoTransaction)
            .await?;
        let mut differences = self
            .load_statement_differences(&items, &mut NoTransaction)
            .await?;
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
        let db = self.db.clone();
        let client = db.client().clone();
        let audit_actor = actor.clone();
        let operation_id = req.operation_id.clone();
        let operation_id_for_tx = operation_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let audit_id_for_tx = audit_id.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_settlement_statements()
                        .update(&mut statement, session)
                        .await?;
                    db.supplier_settlement_differences()
                        .update(&mut difference, session)
                        .await?;
                    let receipt = DifferenceDecisionReceipt {
                        operation_id: operation_id_for_tx,
                        statement_id: statement.base.id.clone(),
                        statement_version: statement.base.version,
                        difference_version: difference.base.version,
                    };
                    let audit = audit_actor.resource_log_with_id(
                        audit_id_for_tx,
                        "supplier_settlement.difference_decision",
                        "supplier_settlement_difference",
                        difference.base.id.clone(),
                        Some(difference_decision_receipt_message(&fingerprint_for_tx, &receipt)),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(SupplierSettlementStatement, SupplierSettlementDifference), crate::errors::Error>((
                        statement, difference,
                    ))
                })
            })
            .await;
        let (statement, difference) = match transaction_result {
            Ok(result) => result,
            Err(error) => {
                if let Some(result) = self
                    .replay_difference_decision(&audit_id, &fingerprint, id)
                    .await?
                {
                    return Ok(result);
                }
                return Err(error);
            }
        };
        Ok(SettlementDifferenceDecisionResult {
            result_status: dto::SettlementDifferenceDecisionStatus::Resolved,
            message: "结算差异正式结论已登记".to_string(),
            operation_id,
            statement_id: statement.base.id,
            statement_lock_version: statement.base.version,
            difference: settlement_difference_view(difference),
        })
    }

    /// 重放差异决定并恢复同一业务结果。
    async fn replay_difference_decision(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        difference_id: &str,
    ) -> Result<Option<SettlementDifferenceDecisionResult>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        ensure_audit_resource(&audit, difference_id)?;
        let receipt = parse_difference_decision_receipt(
            audit.message.as_deref().unwrap_or_default(),
            expected_fingerprint,
        )?;
        let difference = self
            .db
            .supplier_settlement_differences()
            .find_by_id(difference_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("差异决定收据引用的差异不存在".to_string()))?;
        let item = self
            .db
            .supplier_settlement_items()
            .find_by_id(&difference.statement_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("差异决定收据引用的结算明细不存在".to_string()))?;
        if receipt.statement_id != item.statement_id.as_ref()
            || difference.base.version != receipt.difference_version
            || difference.is_pending()
        {
            return Err(Error::ConflictError(
                "差异决定幂等收据与当前正式事实不一致".to_string(),
            ));
        }
        let statement = self.load_statement(&receipt.statement_id).await?;
        if statement.base.version < receipt.statement_version {
            return Err(Error::ConflictError(
                "差异决定幂等收据的结算单版本非法".to_string(),
            ));
        }
        Ok(Some(SettlementDifferenceDecisionResult {
            result_status: dto::SettlementDifferenceDecisionStatus::Resolved,
            message: "结算差异正式结论已登记".to_string(),
            operation_id: receipt.operation_id,
            statement_id: receipt.statement_id,
            statement_lock_version: receipt.statement_version,
            difference: settlement_difference_view(difference),
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DifferenceDecisionReceipt {
    pub operation_id: String,
    pub statement_id: String,
    pub statement_version: u64,
    pub difference_version: u64,
}

/// 将 API 差异决定适配为领域结论类别。
///
/// # 参数
/// * `resolution` - 客户端提交的强类型差异决定
///
/// # 返回
/// 返回实体层用于校验原因、证据和状态推进的结论类别。
fn difference_conclusion_kind(
    resolution: dto::SettlementDifferenceResolution,
) -> SettlementDifferenceConclusionKind {
    match resolution {
        dto::SettlementDifferenceResolution::SupplierAccepted => {
            SettlementDifferenceConclusionKind::SupplierAccepted
        }
        dto::SettlementDifferenceResolution::ErpAccepted => SettlementDifferenceConclusionKind::ErpAccepted,
        dto::SettlementDifferenceResolution::Compensated => SettlementDifferenceConclusionKind::Compensated,
        dto::SettlementDifferenceResolution::ClosedNoAdjustment => {
            SettlementDifferenceConclusionKind::ClosedNoAdjustment
        }
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
fn difference_decision_fingerprint(
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

/// 编码差异决定幂等收据。
pub fn difference_decision_receipt_message(fingerprint: &str, receipt: &DifferenceDecisionReceipt) -> String {
    format!(
        "{COMMAND_FINGERPRINT_PREFIX}{fingerprint};result={}|{}|{}|{}",
        receipt.operation_id, receipt.statement_id, receipt.statement_version, receipt.difference_version,
    )
}

/// 解析并校验差异决定幂等收据。
pub fn parse_difference_decision_receipt(
    message: &str,
    expected_fingerprint: &str,
) -> Result<DifferenceDecisionReceipt> {
    let result = receipt_result(message, expected_fingerprint, "结算差异决定")?;
    let fields = result.split('|').collect::<Vec<_>>();
    let [operation_id, statement_id, statement_version, difference_version] = fields.as_slice() else {
        return Err(Error::Internal("结算差异决定幂等收据结果非法".to_string()));
    };
    Ok(DifferenceDecisionReceipt {
        operation_id: (*operation_id).to_string(),
        statement_id: (*statement_id).to_string(),
        statement_version: parse_receipt_number(statement_version, "结算单版本")?,
        difference_version: parse_receipt_number(difference_version, "差异版本")?,
    })
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
