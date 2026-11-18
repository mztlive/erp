use std::str::FromStr;

use application_core::AuditActor;
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{
    SupplierSettlementDifferenceId, SupplierSettlementItemId, SupplierSettlementStatementId,
};
use id_generator::next_id;
use persistence_core::Executor;
use validator::Validate;

use super::shared::*;
use super::{SettlementAction, SupplierSettlementService};
use crate::dto::supplier_settlement::*;
use crate::entity::supplier_settlement::*;
use crate::repository::SupplierSettlementExt;
use crate::repository::prelude::*;
use crate::{Error, Result};
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
    /// 校验创建请求、动作及结算期间，在原执行器先按确定性单号查重；同供应商、同期间返回已建草稿和当前行
    /// 数，范围不同则冲突。未登记时用该供应商与期间的最新完整来源冻结草稿、明细和差异；没有来源返回业务
    /// 错误，不在此处写入。
    pub async fn prepare_statement(
        &self,
        req: &CreateSettlementStatementRequest,
        actor_id: &str,
        business_org_unit_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<StatementPreparation> {
        req.validate()?;
        let inputs = collect_draft_inputs(req)?;

        if let Some(existing) = self
            .db
            .supplier_settlement_statements()
            .find_by_statement_no(&inputs.statement_no, executor)
            .await?
        {
            validate_create_replay(&existing, req, inputs.period_start, inputs.period_end)?;
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
            .latest_for_scope(&req.supplier_id, inputs.period_start, inputs.period_end, executor)
            .await?
            .ok_or_else(|| {
                Error::BusinessLogicError(
                    "SOURCE_EVIDENCE_MISSING: 当前供应商与期间缺少完整来源证据批次".to_string(),
                )
            })?;
        let prepared = assemble_statement(req, actor_id, business_org_unit_id, &source, inputs)?;
        Ok(StatementPreparation::Ready(prepared))
    }
    // 重读当前明细和差异行数，组装草稿命令结果。
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
    /// 先在原执行器核验刷新权限及实际经办责任；缺结算单返回 `NotFound`，无权操作则拒绝。版本或来源摘要变
    /// 化返回冲突，冻结期间与策略无完整来源时报业务错误。来源相同则沿用原快照和行数；否则准备新金额、明
    /// 细、差异、复核主题及待替换的旧行，不在此处写入。
    pub async fn prepare_refresh(
        &self,
        id: &str,
        req: &RefreshSettlementStatementRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PreparedRefresh> {
        self.access().require_action(actor, SettlementAction::Refresh, id, executor).await?;

        let mut statement = self.load_statement(id, executor).await?;
        statement
            .ensure_version(req.expected_lock_version)
            .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
        if statement.source_snapshot_hash != req.expected_source_snapshot_hash {
            return Err(Error::ConflictError("结算依据已变化，请刷新详情并重新核对结算明细".to_string()));
        }

        let source = self.refresh_source(&statement, executor).await?;
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

        let old_item_ids = old_items.iter().map(|item| item.base.id.clone()).collect::<Vec<_>>();
        let old_difference_ids =
            old_differences.iter().map(|difference| difference.base.id.clone()).collect::<Vec<_>>();
        Ok(PreparedRefresh {
            statement,
            item_count: snapshot.items.len(),
            difference_count: snapshot.differences.len(),
            snapshot: Some(snapshot),
            old_item_ids,
            old_difference_ids,
        })
    }
    // 查询结算单冻结期间和策略对应的完整最新来源证据。
    async fn refresh_source(
        &self,
        statement: &SupplierSettlementStatement,
        executor: &mut dyn Executor,
    ) -> Result<SupplierSettlementSourceEvidence> {
        self.db
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
            })
    }

    /// 按原结算单号查重并只核对供应商和期间，随后重读当前行数。
    /// 没有该结算单号时返回 `None`。供应商和期间一致时返回带当前行数的重放结果。
    /// 已有结算单的供应商或期间不同时返回 `ConflictError`。仓储读取失败时返回对应错误。
    pub async fn replay_statement_create(
        &self,
        req: &CreateSettlementStatementRequest,
        statement_no: &str,
        period_start: BusinessDate,
        period_end: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Option<SettlementDraftCommandResult>> {
        let Some(existing) =
            self.db.supplier_settlement_statements().find_by_statement_no(statement_no, executor).await?
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
    /// 在调用方事务依次写结算单头、冻结明细与差异，使草稿和对账依据一并落库；任一步写入失败即向上传播。
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
    /// 旧草稿已被新快照替换。
    /// 重读时结算单不存在返回 `NotFound`。版本或来源摘要已变化时返回 `ConflictError`。仓储替换失败时返回
    /// 对应错误。
    /// `snapshot`：新的明细和差异快照。
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
            return Err(Error::ConflictError("结算单或结算依据已更新，请刷新后重新核对结算明细".to_string()));
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
/// 已校验的创建草稿输入：期间与确定性结算单号。
struct CollectedDraftInputs {
    /// 结算期间开始。
    period_start: BusinessDate,
    /// 结算期间结束。
    period_end: BusinessDate,
    /// 确定性结算单号。
    statement_no: String,
}

// 校验创建动作与期间并派生确定性结算单号。
// 返回已校验的期间与结算单号。
// 动作非创建、日期非法或期间倒置时返回错误。
fn collect_draft_inputs(req: &CreateSettlementStatementRequest) -> Result<CollectedDraftInputs> {
    if req.action != SettlementDraftAction::Create {
        return Err(Error::ValidationError("请通过新建供应商结算单办理草稿登记".to_string()));
    }
    let period_start = parse_settlement_business_date(&req.period_start, "结算期间开始")?;
    let period_end = parse_settlement_business_date(&req.period_end, "结算期间结束")?;
    if period_end < period_start {
        return Err(Error::ValidationError("结算期间结束不得早于开始".to_string()));
    }
    let statement_no = deterministic_statement_no(req);
    Ok(CollectedDraftInputs { period_start, period_end, statement_no })
}

// 由最新来源快照组装结算单、明细与差异的冻结草稿。
// 返回待持久化的结算单与快照。
// 快照派生或实体构造失败时返回错误。
fn assemble_statement(
    req: &CreateSettlementStatementRequest,
    actor_id: &str,
    business_org_unit_id: &str,
    source: &SupplierSettlementSourceEvidence,
    inputs: CollectedDraftInputs,
) -> Result<PreparedStatement> {
    let statement_id = SupplierSettlementStatementId::new(next_id());
    let snapshot = SupplierSettlementDraftSnapshot::from_source(
        &statement_id,
        source,
        || SupplierSettlementItemId::new(next_id()),
        || SupplierSettlementDifferenceId::new(next_id()),
    )?;

    let now = Instant::now();
    let mut statement = SupplierSettlementStatement::new(
        statement_id,
        draft_statement_data(req, actor_id, business_org_unit_id, source, &snapshot, &inputs, now),
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
    Ok(PreparedStatement {
        statement,
        snapshot,
        statement_no: inputs.statement_no,
        period_start: inputs.period_start,
        period_end: inputs.period_end,
    })
}

// 由请求、来源与快照组装结算单初始数据。
// 返回可直接构造结算单实体的数据。
// `snapshot`：已派生的草稿快照。
fn draft_statement_data(
    req: &CreateSettlementStatementRequest,
    actor_id: &str,
    business_org_unit_id: &str,
    source: &SupplierSettlementSourceEvidence,
    snapshot: &SupplierSettlementDraftSnapshot,
    inputs: &CollectedDraftInputs,
    now: Instant,
) -> SupplierSettlementStatementData {
    SupplierSettlementStatementData {
        statement_no: inputs.statement_no.clone(),
        supplier_id: req.supplier_id.clone(),
        period_start: inputs.period_start,
        period_end: inputs.period_end,
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
        business_org_unit_id: business_org_unit_id.to_string(),
        difference_handler_user_id: actor_id.to_string(),
    }
}

// 结算单号取期间结束文本的前 6 位数字，再接请求身份摘要的前 16 位。
fn deterministic_statement_no(req: &CreateSettlementStatementRequest) -> String {
    let digest = digest_parts(&[
        "supplier-settlement-create-v1".to_string(),
        req.request_id.clone(),
        req.idempotency_key.clone(),
    ]);
    let month = req.period_end.chars().filter(char::is_ascii_digit).take(6).collect::<String>();
    format!("ST-{month}-{}", &digest[..16])
}

// 同一结算单号只接受相同供应商和期间，其余载荷不在此比较。
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
            "本次结算单创建内容与之前提交的不一致，请先查看原创建结果".to_string(),
        ));
    }
    Ok(())
}

// 解析结算业务日期文本（草稿创建与来源录入共用同一口径）。
// 返回强类型业务日期。
// 日期格式非法时返回 `ValidationError`。
pub(super) fn parse_settlement_business_date(value: &str, field: &str) -> Result<BusinessDate> {
    BusinessDate::from_str(value.trim())
        .map_err(|_| Error::ValidationError(format!("{field}不是合法业务日期")))
}
