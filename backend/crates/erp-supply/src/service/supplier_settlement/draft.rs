use super::shared::*;
use super::SupplierSettlementService;
use crate::dto::supplier_settlement::*;
use crate::entity::supplier_settlement::*;
use crate::repository::SupplierSettlementExt;
use crate::{Error, Result};
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{
    SupplierSettlementDifferenceId, SupplierSettlementItemId, SupplierSettlementStatementId,
};
use id_generator::next_id;
use persistence_core::Executor;
use std::str::FromStr;
use validator::Validate;
/// 已核验的创建结算单与冻结来源快照。
pub struct PreparedStatement {
    pub statement: SupplierSettlementStatement,
    pub snapshot: SupplierSettlementDraftSnapshot,
    pub statement_no: String,
    pub period_start: BusinessDate,
    pub period_end: BusinessDate,
}
/// 创建请求可重放既有结果，或进入原创建根事务。
pub enum StatementPreparation {
    Replay(SettlementDraftCommandResult),
    Ready(PreparedStatement),
}
/// 已读完原来源/旧明细的刷新准备；None 快照表示原相同来源 no-op。
pub struct PreparedRefresh {
    pub statement: SupplierSettlementStatement,
    pub snapshot: Option<SupplierSettlementDraftSnapshot>,
    pub old_item_ids: Vec<String>,
    pub old_difference_ids: Vec<String>,
    pub item_count: usize,
    pub difference_count: usize,
}
impl SupplierSettlementService {
    /// 核验创建动作、期间和幂等记录，再从最新来源依原顺序构造结算单与快照。
    pub async fn prepare_statement(
        &self,
        req: &CreateSettlementStatementRequest,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<StatementPreparation> {
        req.validate()?;
        if req.action != SettlementDraftAction::Create {
            return Err(Error::ValidationError(
                "创建结算草稿必须使用 CREATE 动作".to_string(),
            ));
        }
        let period_start = parse_business_date(&req.period_start, "结算期间开始")?;
        let period_end = parse_business_date(&req.period_end, "结算期间结束")?;
        if period_end < period_start {
            return Err(Error::ValidationError("结算期间结束不得早于开始".to_string()));
        }
        let statement_no = deterministic_statement_no(req);
        if let Some(existing) = self
            .db
            .supplier_settlement_statements()
            .find_by_statement_no(&statement_no, executor)
            .await?
        {
            validate_create_replay(&existing, req, period_start, period_end)?;
            return Ok(StatementPreparation::Replay(
                self.draft_result(
                    existing,
                    req.request_id.clone(),
                    "REPLAYED",
                    "结算草稿创建结果已恢复",
                    executor,
                )
                .await?,
            ));
        }
        let source = self
            .db
            .supplier_settlement_source_evidence()
            .latest_for_scope(&req.supplier_id, period_start, period_end, executor)
            .await?
            .ok_or_else(|| {
                Error::BusinessLogicError(
                    "SOURCE_EVIDENCE_MISSING: 当前供应商与期间缺少完整来源证据批次".to_string(),
                )
            })?;
        let statement_id = SupplierSettlementStatementId::new(next_id());
        let snapshot = SupplierSettlementDraftSnapshot::from_source(
            &statement_id,
            &source,
            || SupplierSettlementItemId::new(next_id()),
            || SupplierSettlementDifferenceId::new(next_id()),
        )?;
        let now = Instant::now();
        let mut statement = SupplierSettlementStatement::new(
            statement_id,
            SupplierSettlementStatementData {
                statement_no: statement_no.clone(),
                supplier_id: req.supplier_id.clone(),
                period_start,
                period_end,
                period_policy_id: source.period_policy_id.clone(),
                period_policy_version: source.period_policy_version.clone(),
                period_timezone: source.timezone.clone(),
                external_bill_no: Some(source.external_bill_no.clone()),
                external_bill_version: Some(source.external_bill_version.clone()),
                erp_amount: snapshot.erp_amount,
                supplier_amount: snapshot.supplier_amount,
                subject_hash: "0".repeat(64),
                source_as_of: source.source_as_of,
                source_snapshot_at: now,
                source_snapshot_hash: source.source_hash.clone(),
                refresh_cutoff_policy_id: REVIEW_CUTOFF_POLICY_ID.to_string(),
                refresh_cutoff_policy_version: REVIEW_CUTOFF_POLICY_VERSION.to_string(),
                prepared_by: actor_id.to_string(),
            },
        )?;
        statement.refresh_snapshot(SupplierSettlementSnapshotUpdate {
            external_bill_no: source.external_bill_no.clone(),
            external_bill_version: source.external_bill_version.clone(),
            erp_amount: snapshot.erp_amount,
            supplier_amount: snapshot.supplier_amount,
            source_as_of: source.source_as_of,
            source_snapshot_at: now,
            source_snapshot_hash: source.source_hash.clone(),
            has_difference: !snapshot.differences.is_empty(),
        })?;
        statement.update_subject_hash(statement.review_subject_hash(&snapshot.differences))?;

        Ok(StatementPreparation::Ready(PreparedStatement {
            statement,
            snapshot,
            statement_no,
            period_start,
            period_end,
        }))
    }
    async fn draft_result(
        &self,
        statement: SupplierSettlementStatement,
        request_id: String,
        result_status: &str,
        message: &str,
        executor: &mut dyn Executor,
    ) -> Result<SettlementDraftCommandResult> {
        let items = self.load_statement_items(&statement.base.id, executor).await?;
        let differences = self.load_statement_differences(&items, executor).await?;
        Ok(SettlementDraftCommandResult {
            result_status: result_status.to_string(),
            message: message.to_string(),
            request_id,
            statement: statement.into(),
            item_count: items.len(),
            difference_count: differences.len(),
        })
    }
    /// 按责任、版本、来源和旧明细的原顺序准备刷新；相同来源不创建新快照。
    pub async fn prepare_refresh(
        &self,
        id: &str,
        req: &RefreshSettlementStatementRequest,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<PreparedRefresh> {
        let mut statement = self.load_statement(id, executor).await?;
        if statement.prepared_by != actor_id {
            return Err(Error::Forbidden("只有当前结算经办人可以刷新试算".to_string()));
        }
        statement
            .ensure_version(req.expected_lock_version)
            .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
        if statement.source_snapshot_hash != req.expected_source_snapshot_hash {
            return Err(Error::ConflictError(
                "结算来源快照已变化，请刷新详情后重试".to_string(),
            ));
        }
        let source = self
            .db
            .supplier_settlement_source_evidence()
            .latest_for_period(
                &statement.supplier_id,
                statement.period_start,
                statement.period_end,
                &statement.period_policy_id,
                &statement.period_policy_version,
                executor,
            )
            .await?
            .ok_or_else(|| {
                Error::BusinessLogicError(
                    "SOURCE_EVIDENCE_MISSING: 冻结策略版本缺少完整来源证据批次".to_string(),
                )
            })?;
        let old_items = self.load_statement_items(id, executor).await?;
        let old_differences = self.load_statement_differences(&old_items, executor).await?;
        if source.source_hash == statement.source_snapshot_hash {
            return Ok(PreparedRefresh {
                statement,
                snapshot: None,
                old_item_ids: Vec::new(),
                old_difference_ids: Vec::new(),
                item_count: old_items.len(),
                difference_count: old_differences.len(),
            });
        }
        let snapshot = SupplierSettlementDraftSnapshot::from_source(
            &SupplierSettlementStatementId::new(statement.base.id.clone()),
            &source,
            || SupplierSettlementItemId::new(next_id()),
            || SupplierSettlementDifferenceId::new(next_id()),
        )?;
        statement.refresh_snapshot(SupplierSettlementSnapshotUpdate {
            external_bill_no: source.external_bill_no.clone(),
            external_bill_version: source.external_bill_version.clone(),
            erp_amount: snapshot.erp_amount,
            supplier_amount: snapshot.supplier_amount,
            source_as_of: source.source_as_of,
            source_snapshot_at: Instant::now(),
            source_snapshot_hash: source.source_hash.clone(),
            has_difference: !snapshot.differences.is_empty(),
        })?;
        statement.update_subject_hash(statement.review_subject_hash(&snapshot.differences))?;
        let old_item_ids = old_items
            .iter()
            .map(|item| item.base.id.clone())
            .collect::<Vec<_>>();
        let old_difference_ids = old_differences
            .iter()
            .map(|difference| difference.base.id.clone())
            .collect::<Vec<_>>();
        Ok(PreparedRefresh {
            statement,
            item_count: snapshot.items.len(),
            difference_count: snapshot.differences.len(),
            snapshot: Some(snapshot),
            old_item_ids,
            old_difference_ids,
        })
    }
    /// 按原结算单号查重并只核对供应商和期间，随后重读当前行数。
    pub async fn replay_statement_create(
        &self,
        req: &CreateSettlementStatementRequest,
        statement_no: &str,
        period_start: BusinessDate,
        period_end: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Option<SettlementDraftCommandResult>> {
        let Some(existing) = self
            .db
            .supplier_settlement_statements()
            .find_by_statement_no(statement_no, executor)
            .await?
        else {
            return Ok(None);
        };
        validate_create_replay(&existing, req, period_start, period_end)?;
        Ok(Some(
            self.draft_result(
                existing,
                req.request_id.clone(),
                "REPLAYED",
                "结算草稿创建结果已恢复",
                executor,
            )
            .await?,
        ))
    }
    /// 按原仓储的单头、明细、差异顺序创建冻结结算快照。
    pub async fn persist_statement_with_items(
        &self,
        statement: &SupplierSettlementStatement,
        items: &[SupplierSettlementItem],
        differences: &[SupplierSettlementDifference],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db
            .supplier_settlement()
            .create_statement_with_items(statement, items, differences, executor)
            .await?;
        Ok(())
    }
    /// 在调用者事务内重验旧版本和来源摘要，再按原物理顺序替换草稿。
    pub async fn persist_refreshed_statement(
        &self,
        statement: &mut SupplierSettlementStatement,
        snapshot: &SupplierSettlementDraftSnapshot,
        old_ids: (&[String], &[String]),
        req: &RefreshSettlementStatementRequest,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let current = self.load_statement(&statement.base.id, executor).await?;
        if current.base.version != req.expected_lock_version
            || current.source_snapshot_hash != req.expected_source_snapshot_hash
        {
            return Err(Error::ConflictError(
                "结算单版本或来源快照已变化，请刷新后重试".to_string(),
            ));
        }
        self.db
            .supplier_settlement()
            .replace_draft_snapshot(
                statement,
                old_ids.0,
                old_ids.1,
                &snapshot.items,
                &snapshot.differences,
                executor,
            )
            .await?;
        Ok(())
    }
}
fn deterministic_statement_no(req: &CreateSettlementStatementRequest) -> String {
    let digest = digest_parts(&[
        "supplier-settlement-create-v1".to_string(),
        req.request_id.clone(),
        req.idempotency_key.clone(),
    ]);
    let month = req
        .period_end
        .chars()
        .filter(char::is_ascii_digit)
        .take(6)
        .collect::<String>();
    format!("ST-{month}-{}", &digest[..16])
}

fn validate_create_replay(
    statement: &SupplierSettlementStatement,
    req: &CreateSettlementStatementRequest,
    period_start: BusinessDate,
    period_end: BusinessDate,
) -> Result<()> {
    if statement.supplier_id != req.supplier_id
        || statement.period_start != period_start
        || statement.period_end != period_end
    {
        return Err(Error::ConflictError(
            "创建幂等键已用于不同的供应商结算命令".to_string(),
        ));
    }
    Ok(())
}

fn parse_business_date(value: &str, field: &str) -> Result<BusinessDate> {
    BusinessDate::from_str(value.trim())
        .map_err(|_| Error::ValidationError(format!("{field}不是合法业务日期")))
}
