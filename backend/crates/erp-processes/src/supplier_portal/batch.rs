//! 整批预检、逐单元事务及原命令恢复；新品先准备原草稿，独立补图后批量提交原申请。

use std::collections::{HashMap, HashSet};

use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::WorkItemId;
use erp_identity::PortalActor;
use erp_supply::portal::{
    ApplicationKind, OfferingApplication, OfferingApplicationSnapshot, PortalAvailabilityInput,
    PortalOfferingService, PortalQuoteInput,
};
use erp_workflow::entity::work_item::SupplierPortalReviewTaskData;
use id_generator::next_id;
use persistence_core::{Executor, NoTransaction};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::command::scoped_command;
use super::command_recovery::{CommandRejection, rejection};
use super::new_products::SupplyTerms;
use super::offerings::offering_subject;
use super::{NewProductSave, PortalTransition, SupplierPortalProcess};
use crate::adapters::workflow::work_item_service;
use crate::{Error, Result};

/// 批量模式分别约束直接可供维护和两条申请入口。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PortalBatchMode {
    Availability,
    Quote,
    Terms,
    NewProduct,
}
/// 新品准备与最终提报分别保留稳定命令身份。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PortalBatchPhase {
    Prepare,
    Submit,
}
/// 一行的原始命令内容；网络中断后必须完整保留。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortalBatchRow {
    pub row_id: String,
    pub idempotency_key: String,
    pub input: Value,
}
/// 最多100条SKU/供给行，恢复优先返回回执，无回执按原内容及原操作号重试。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortalBatchInput {
    pub mode: PortalBatchMode,
    pub phase: Option<PortalBatchPhase>,
    pub validate_only: bool,
    #[serde(default)]
    pub recovery_only: bool,
    pub rows: Vec<PortalBatchRow>,
}
/// 一行的实际结果，外部只暴露必要申请/供给身份。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortalBatchRowResult {
    pub row_id: String,
    pub status: String,
    pub error: Option<String>,
    pub result: Option<Value>,
}
/// 批量整批校验和各单元执行结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortalBatchResult {
    pub valid: bool,
    pub rows: Vec<PortalBatchRowResult>,
}

#[derive(Clone)]
enum PreparedInput {
    Availability(String, PortalAvailabilityInput),
    Application(PortalQuoteInput),
    NewProduct(NewProductSave),
    NewProductSubmit(String, PortalTransition),
}
struct PreparedRow {
    row: PortalBatchRow,
    input: PreparedInput,
    replay: Option<Value>,
    rejection: Option<CommandRejection>,
    sku_count: usize,
    business_units: Vec<String>,
}

impl SupplierPortalProcess {
    /// 先预检整批，再对各行独立提交；恢复重用原内容及原操作号。
    /// # 参数
    /// 当前门户身份和保留原命令标识的批量请求。
    /// # 返回
    /// 返回原顺序逐行结果；新品准备返回草稿，最终提交返回带具体任务的原申请。
    /// # 错误
    /// 批次数量、身份或归属输入非法时拒绝；行失败保留成功行。
    pub async fn batch(&self, actor: &PortalActor, input: PortalBatchInput) -> Result<PortalBatchResult> {
        actor.require_write()?;
        self.session_validate(actor, &mut NoTransaction).await?;
        validate_rows(&input)?;
        let (prepared, mut results) = self.preflight_batch(actor, &input).await;
        let valid = batch_rows_valid(&results);
        if !valid || input.validate_only {
            retain_unresolved_recovery(&input, valid, &mut results);
            return Ok(PortalBatchResult { valid: batch_rows_valid(&results), rows: results });
        }
        let mut rows = Vec::new();
        for plan in prepared {
            if let Some(result) = plan.replay {
                rows.push(row_result(&plan.row.row_id, "replayed", None, Some(external_result(result))));
                continue;
            }
            rows.push(self.run_prepared_row(actor, plan, input.recovery_only).await);
        }
        Ok(PortalBatchResult { valid: true, rows })
    }

    async fn preflight_batch(
        &self,
        actor: &PortalActor,
        input: &PortalBatchInput,
    ) -> (Vec<PreparedRow>, Vec<PortalBatchRowResult>) {
        let duplicates = duplicate_business_units(input);
        let mut prepared = Vec::new();
        let mut results = Vec::new();
        for row in &input.rows {
            match self
                .prepare_batch_row(
                    actor,
                    input.mode,
                    input.phase,
                    row,
                    input.recovery_only && !input.validate_only,
                )
                .await
            {
                Ok(plan) => {
                    if let Some(rejection) = &plan.rejection {
                        results.push(rejected_row_result(&row.row_id, rejection));
                        continue;
                    }
                    if plan.replay.is_none()
                        && let Some(message) = duplicates.get(&row.row_id)
                    {
                        results.push(row_result(
                            &row.row_id,
                            "validation_failed",
                            Some(message.clone()),
                            None,
                        ));
                        continue;
                    }
                    results.push(row_result(
                        &row.row_id,
                        if plan.replay.is_some() { "replayed" } else { "valid" },
                        None,
                        plan.replay.clone().map(external_result),
                    ));
                    prepared.push(plan);
                },
                Err(error) => {
                    let status = if input.recovery_only && parse_row(input.mode, input.phase, row).is_ok() {
                        "unknown"
                    } else {
                        "validation_failed"
                    };
                    results.push(row_result(&row.row_id, status, Some(error.to_string()), None))
                },
            }
        }
        validate_prepared_rows(&prepared, &mut results);
        (prepared, results)
    }

    async fn run_prepared_row(
        &self,
        actor: &PortalActor,
        plan: PreparedRow,
        recovery_only: bool,
    ) -> PortalBatchRowResult {
        let result = self.execute_batch_row(actor, plan.input, &plan.row).await;
        execution_row_result(&plan.row.row_id, recovery_only, result)
    }

    async fn prepare_batch_row(
        &self,
        actor: &PortalActor,
        mode: PortalBatchMode,
        phase: Option<PortalBatchPhase>,
        row: &PortalBatchRow,
        recover: bool,
    ) -> Result<PreparedRow> {
        let input = parse_row(mode, phase, row)?;
        let action = batch_action(&input);
        let command =
            scoped_command(&actor.account_id, &actor.supplier_id, action, &row.idempotency_key, &row.input)?;
        let service = PortalOfferingService::new(self.db.clone());
        let mut replay = service.command_result(&command, &mut NoTransaction).await?;
        if replay.is_none()
            && let Err(error) = self.preflight_batch_input(actor, &input, &mut NoTransaction).await
        {
            if recover {
                let check = input.clone();
                replay = self
                    .recover_unexecutable(
                        actor,
                        action,
                        &row.idempotency_key,
                        &row.input,
                        move |this, actor, executor| {
                            Box::pin(
                                async move { this.preflight_batch_input(&actor, &check, executor).await },
                            )
                        },
                    )
                    .await?;
            } else {
                replay = service.command_result(&command, &mut NoTransaction).await?;
                if replay.is_none() {
                    return Err(error);
                }
            }
        }
        let rejection = replay.as_ref().map(rejection).transpose()?.flatten();
        let (sku_count, business_units) =
            if rejection.is_some() { (0, Vec::new()) } else { self.batch_units(actor, &input).await? };
        Ok(PreparedRow { row: row.clone(), input, replay, rejection, sku_count, business_units })
    }

    async fn batch_units(&self, actor: &PortalActor, input: &PreparedInput) -> Result<(usize, Vec<String>)> {
        if let PreparedInput::NewProductSubmit(id, _) = input {
            let draft = self.catalog_portal().detail(id, &actor.supplier_id, &mut NoTransaction).await?;
            let mut units = vec![format!("request:{}", id.trim())];
            units.extend(
                draft.draft.skus.iter().map(|row| format!("ordering_code:{}", row.ordering_code.trim())),
            );
            return Ok((draft.draft.skus.len(), units));
        }
        let count = match input {
            PreparedInput::NewProduct(req) => req.input.skus.len(),
            _ => 1,
        };
        Ok((count, business_units(input)))
    }

    async fn preflight_batch_input(
        &self,
        actor: &PortalActor,
        input: &PreparedInput,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let service = PortalOfferingService::new(self.db.clone());
        match input {
            PreparedInput::Availability(id, req) => {
                service
                    .validate_availability(&actor.supplier_id, &actor.audit_actor(), id, req, executor)
                    .await?;
                self.validate_availability_unit(actor, id, req, executor).await?;
            },
            PreparedInput::Application(req) => {
                self.preflight_batch_application(actor, req, executor).await?;
            },
            PreparedInput::NewProduct(req) => {
                self.catalog_portal().validate_for_submission(&req.input, &SupplyTerms, executor).await?;
                self.validate_assets(&actor.supplier_id, None, &req.input, false, executor).await?;
            },
            PreparedInput::NewProductSubmit(id, req) => {
                self.preflight_new_product_submission(actor, id, req, executor).await?;
            },
        }
        Ok(())
    }

    async fn preflight_batch_application(
        &self,
        actor: &PortalActor,
        req: &PortalQuoteInput,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let service = PortalOfferingService::new(self.db.clone());
        let snapshot = service
            .prepare_snapshot(
                &actor.supplier_id,
                &actor.audit_actor(),
                req.snapshot.clone(),
                BusinessDate::today(),
                executor,
            )
            .await?;
        self.validate_snapshot(actor, &snapshot, executor).await?;
        let app = OfferingApplication::new(
            "batch-preflight".into(),
            &actor.supplier_id,
            &actor.audit_actor(),
            snapshot,
            &req.reason,
        )?;
        self.batch_offering_reviewer(&app, actor, executor).await.map_err(qualification_error)?;
        Ok(())
    }

    async fn preflight_new_product_submission(
        &self,
        actor: &PortalActor,
        id: &str,
        input: &PortalTransition,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let service = self.catalog_portal();
        let mut draft = service.detail(id, &actor.supplier_id, executor).await?;
        draft.draft.ensure_submission_images()?;
        service.validate_for_submission(&draft.draft, &SupplyTerms, executor).await?;
        self.validate_assets(&actor.supplier_id, Some(id), &draft.draft, false, executor).await?;
        self.batch_new_product_reviewer(&draft, executor).await.map_err(qualification_error)?;
        draft.submit(
            input.expected_version,
            "batch-preflight-submission".into(),
            "batch-preflight-task".into(),
            actor.account_id.clone(),
            Instant::now(),
        )?;
        Ok(())
    }

    async fn execute_batch_row(
        &self,
        actor: &PortalActor,
        input: PreparedInput,
        row: &PortalBatchRow,
    ) -> Result<Value> {
        let action = batch_action(&input);
        self.portal_command(actor, action, &row.idempotency_key, &row.input, move |this, actor, executor| {
            Box::pin(async move {
                match input {
                    PreparedInput::Availability(id, req) => {
                        this.validate_availability_unit(&actor, &id, &req, executor).await?;
                        let result = PortalOfferingService::new(this.db)
                            .update_availability_with_change(
                                &actor.supplier_id,
                                &actor.audit_actor(),
                                &id,
                                &req,
                                executor,
                            )
                            .await?;
                        serialize(result)
                    },
                    PreparedInput::Application(req) => this.batch_application(&actor, req, executor).await,
                    PreparedInput::NewProduct(req) => this.batch_new_product(&actor, req, executor).await,
                    PreparedInput::NewProductSubmit(id, req) => {
                        serialize(this.submit_new_product(&id, &req, &actor, executor).await?)
                    },
                }
            })
        })
        .await
    }

    async fn batch_application(
        &self,
        actor: &PortalActor,
        req: PortalQuoteInput,
        executor: &mut dyn Executor,
    ) -> Result<Value> {
        let service = PortalOfferingService::new(self.db.clone());
        let snapshot = service
            .prepare_snapshot(
                &actor.supplier_id,
                &actor.audit_actor(),
                req.snapshot,
                BusinessDate::today(),
                executor,
            )
            .await?;
        self.validate_snapshot(actor, &snapshot, executor).await?;
        let mut app = OfferingApplication::new(
            next_id(),
            &actor.supplier_id,
            &actor.audit_actor(),
            snapshot,
            &req.reason,
        )?;
        let (owner, org) = self.application_owner(&app, actor, executor).await?;
        let task_id = next_id();
        app.submit(&actor.audit_actor(), &owner, &task_id, 1, Instant::now())?;
        service.save_application(&mut app, true, executor).await?;
        work_item_service(self.db.clone(), self.rbac.clone())
            .create_supplier_portal_review(
                WorkItemId::new(task_id),
                SupplierPortalReviewTaskData {
                    request_id: app.base.id.clone(),
                    subject_version: offering_subject(&app)?,
                    owner_user_id: owner,
                    owner_organization_id: org,
                    due_at: None,
                    impact_summary: None,
                },
                executor,
            )
            .await?;
        serialize(app)
    }

    async fn batch_new_product(
        &self,
        actor: &PortalActor,
        req: NewProductSave,
        executor: &mut dyn Executor,
    ) -> Result<Value> {
        self.validate_assets(&actor.supplier_id, None, &req.input, true, executor).await?;
        let service = self.catalog_portal();
        service.validate_for_submission(&req.input, &SupplyTerms, executor).await?;
        let draft = service
            .create(next_id(), actor.supplier_id.clone(), actor.account_id.clone(), req.input, executor)
            .await?;
        serialize(draft)
    }
}

fn parse_row(
    mode: PortalBatchMode,
    phase: Option<PortalBatchPhase>,
    row: &PortalBatchRow,
) -> Result<PreparedInput> {
    let mut input = row.input.clone();
    let map = input.as_object_mut().ok_or_else(|| Error::ValidationError("批量行必须是对象".into()))?;
    map.insert("idempotency_key".into(), Value::String(row.idempotency_key.clone()));
    match mode {
        PortalBatchMode::Availability => {
            let id = map
                .remove("offering_id")
                .and_then(|v| v.as_str().map(str::to_string))
                .ok_or_else(|| Error::ValidationError("可供行缺少供给标识".into()))?;
            Ok(PreparedInput::Availability(id, deserialize(input)?))
        },
        PortalBatchMode::Quote | PortalBatchMode::Terms => {
            let quote: PortalQuoteInput = deserialize(input)?;
            let valid = matches!(
                (mode, quote.snapshot.kind()),
                (PortalBatchMode::Quote, ApplicationKind::ExistingQuote)
                    | (PortalBatchMode::Terms, ApplicationKind::TermsChange)
            );
            if !valid {
                return Err(Error::ValidationError("批量模式与申请类型不符".into()));
            }
            Ok(PreparedInput::Application(quote))
        },
        PortalBatchMode::NewProduct => match phase {
            Some(PortalBatchPhase::Prepare) => Ok(PreparedInput::NewProduct(deserialize(input)?)),
            Some(PortalBatchPhase::Submit) => {
                let id = map
                    .remove("id")
                    .and_then(|value| value.as_str().map(str::to_string))
                    .filter(|id| !id.trim().is_empty())
                    .ok_or_else(|| Error::ValidationError("新品批量提交必须指定原草稿标识".into()))?;
                Ok(PreparedInput::NewProductSubmit(id, deserialize(input)?))
            },
            None => Err(Error::ValidationError("新品批量必须明确准备或最终提交阶段".into())),
        },
    }
}
fn validate_rows(input: &PortalBatchInput) -> Result<()> {
    if (input.mode == PortalBatchMode::NewProduct) != input.phase.is_some() {
        return Err(Error::ValidationError("仅新品批量必须明确准备或最终提交阶段".into()));
    }
    if input.rows.is_empty() || input.rows.len() > 100 {
        return Err(Error::ValidationError("每批必须包含1至100条SKU或供给数据".into()));
    }
    let mut rows = HashSet::new();
    let mut keys = HashSet::new();
    let mut count = 0usize;
    for row in &input.rows {
        if row.row_id.trim().is_empty()
            || row.idempotency_key.trim().is_empty()
            || !rows.insert(row.row_id.trim())
            || !keys.insert(row.idempotency_key.trim())
        {
            return Err(Error::ValidationError("行号及操作号必须非空且不可重复".into()));
        }
        count +=
            if input.mode == PortalBatchMode::NewProduct && input.phase == Some(PortalBatchPhase::Prepare) {
                row.input.pointer("/input/skus").and_then(Value::as_array).map_or(0, Vec::len)
            } else {
                1
            };
    }
    if count == 0 || count > 100 {
        return Err(Error::ValidationError("每批最多100条SKU或供给行".into()));
    }
    Ok(())
}
fn duplicate_business_units(input: &PortalBatchInput) -> HashMap<String, String> {
    let mut units = HashMap::new();
    let mut errors = HashMap::new();
    for row in &input.rows {
        let Ok(parsed) = parse_row(input.mode, input.phase, row) else { continue };
        for unit in business_units(&parsed) {
            if let Some(previous) = units.insert(unit.clone(), row.row_id.clone()) {
                let message = format!("行{previous}与行{}重复业务单元{unit}，必须合并后再提交", row.row_id);
                errors.insert(previous, message.clone());
                errors.insert(row.row_id.clone(), message);
            }
        }
    }
    errors
}
fn business_units(input: &PreparedInput) -> Vec<String> {
    match input {
        PreparedInput::Availability(id, _) => vec![format!("offering:{}", id.trim())],
        PreparedInput::Application(req) => match &req.snapshot {
            OfferingApplicationSnapshot::ExistingQuote { supplier_sku_code, .. } => {
                vec![format!("ordering_code:{}", supplier_sku_code.trim())]
            },
            snapshot => snapshot
                .target()
                .map(|(id, _, _)| vec![format!("offering:{}", id.trim())])
                .unwrap_or_default(),
        },
        PreparedInput::NewProduct(req) => {
            req.input.skus.iter().map(|sku| format!("ordering_code:{}", sku.ordering_code.trim())).collect()
        },
        PreparedInput::NewProductSubmit(id, _) => vec![format!("request:{}", id.trim())],
    }
}
fn batch_action(input: &PreparedInput) -> &'static str {
    match input {
        PreparedInput::Availability(..) => "supplier_portal.availability_update",
        PreparedInput::Application(_) => "supplier_portal.application_submit",
        PreparedInput::NewProduct(_) => "supplier_portal.new_product_save",
        PreparedInput::NewProductSubmit(..) => "supplier_portal.new_product_submit",
    }
}
fn validate_prepared_rows(prepared: &[PreparedRow], results: &mut [PortalBatchRowResult]) {
    let mut units = HashMap::new();
    let mut errors = HashMap::new();
    for plan in prepared.iter().filter(|plan| plan.replay.is_none()) {
        for unit in &plan.business_units {
            if let Some(previous) = units.insert(unit.clone(), plan.row.row_id.clone()) {
                let message =
                    format!("行{previous}与行{}重复业务单元{unit}，必须合并后再提交", plan.row.row_id);
                errors.insert(previous, message.clone());
                errors.insert(plan.row.row_id.clone(), message);
            }
        }
    }
    let oversized = prepared.iter().map(|plan| plan.sku_count).sum::<usize>() > 100;
    for row in results {
        if !matches!(row.status.as_str(), "valid" | "validation_failed") {
            continue;
        }
        let error = if oversized {
            Some("每批最多100条实际SKU或供给行".to_string())
        } else {
            errors.remove(&row.row_id)
        };
        if let Some(error) = error {
            row.status = "validation_failed".into();
            row.error = Some(error);
        }
    }
}
fn deserialize<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(|e| Error::ValidationError(e.to_string()))
}
fn serialize<T: Serialize>(result: T) -> Result<Value> {
    serde_json::to_value(result).map_err(|e| Error::Internal(e.to_string()))
}
fn row_result(id: &str, status: &str, error: Option<String>, result: Option<Value>) -> PortalBatchRowResult {
    PortalBatchRowResult { row_id: id.into(), status: status.into(), error, result }
}
fn rejected_row_result(id: &str, rejection: &CommandRejection) -> PortalBatchRowResult {
    row_result(
        id,
        "failed",
        Some(rejection.clone().into_error().to_string()),
        Some(json!({"command_outcome":"rejected"})),
    )
}
/// 恢复重试失败只能说明本次未完成，不能排除更早原请求仍在提交。
fn execution_row_result(id: &str, recovery_only: bool, result: Result<Value>) -> PortalBatchRowResult {
    match result {
        Ok(value) => row_result(id, "succeeded", None, Some(external_result(value))),
        Err(error) => row_result(
            id,
            if recovery_only || error.command_may_have_committed() { "unknown" } else { "failed" },
            Some(error.to_string()),
            None,
        ),
    }
}
fn batch_rows_valid(rows: &[PortalBatchRowResult]) -> bool {
    rows.iter().all(|row| matches!(row.status.as_str(), "valid" | "replayed"))
}
/// 处理资格中的配置或策略失败属于授权失败，不得作为原业务命令的确定拒绝封存。
fn qualification_error(error: Error) -> Error {
    match error {
        Error::ValidationError(_) | Error::BusinessLogicError(_) | Error::ConflictError(_) => {
            Error::Rbac(format!("申请处理资格无法确认：{error}"))
        },
        error => error,
    }
}
/// 本次未执行的预检不能证明旧请求未提交；只有成功或拒绝回执可以解除未知状态。
fn retain_unresolved_recovery(input: &PortalBatchInput, valid: bool, rows: &mut [PortalBatchRowResult]) {
    if !input.recovery_only || (valid && !input.validate_only) {
        return;
    }
    for row in rows {
        if matches!(row.status.as_str(), "valid" | "validation_failed") {
            row.status = "unknown".into();
            row.result = None;
            row.error.get_or_insert_with(|| "原命令尚无终态回执，请保留原内容及原操作号恢复".into());
        }
    }
}
fn external_result(value: Value) -> Value {
    json!({"id":value.get("id").or_else(||value.get("offering_id")),"version":value.get("version").or_else(||value.get("availability_version")),"status":value.get("status").or_else(||value.get("availability_status")),"result":value.get("result")})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn batch(rows: Vec<PortalBatchRow>) -> PortalBatchInput {
        PortalBatchInput {
            mode: PortalBatchMode::Availability,
            phase: None,
            validate_only: false,
            recovery_only: false,
            rows,
        }
    }
    fn row(id: &str, key: &str) -> PortalBatchRow {
        PortalBatchRow {
            row_id: id.into(),
            idempotency_key: key.into(),
            input: json!({"offering_id":"offering-1","expected_version":1,"availability_status":"AVAILABLE","available_quantity":null,"reason":"报送"}),
        }
    }
    #[test]
    fn batch_shape_rejects_empty_duplicate_keys_and_rows() {
        assert!(validate_rows(&batch(Vec::new())).is_err());
        assert!(validate_rows(&batch(vec![row("1", "a"), row("2", "a")])).is_err());
        assert!(validate_rows(&batch(vec![row("1", "a"), row("1", "b")])).is_err());
        assert!(validate_rows(&batch(vec![row("1", "a")])).is_ok());
    }
    #[test]
    fn row_parser_keeps_blank_quantity_and_original_command() {
        let PreparedInput::Availability(id, req) =
            parse_row(PortalBatchMode::Availability, None, &row("1", "original-key")).unwrap()
        else {
            panic!("wrong row")
        };
        assert_eq!(id, "offering-1");
        assert_eq!(req.available_quantity, None);
        assert_eq!(req.idempotency_key, "original-key");
    }
    #[test]
    fn distinct_command_keys_cannot_write_the_same_business_unit_twice() {
        let input = batch(vec![row("row-a", "key-a"), row("row-b", "key-b")]);
        validate_rows(&input).unwrap();
        let duplicates = duplicate_business_units(&input);
        assert_eq!(duplicates.len(), 2);
        assert!(duplicates["row-a"].contains("row-b"));
        assert!(duplicates["row-b"].contains("offering-1"));
        let mut independent = input;
        independent.rows[1].input["offering_id"] = json!("offering-2");
        assert!(duplicate_business_units(&independent).is_empty());
    }
    fn quote_row(id: &str, key: &str, code: &str) -> PortalBatchRow {
        let terms = json!({"dropship_supply_price_gross":"11.30","bulk_supply_price_gross":"9.04","input_tax_rate":"0.13","bulk_minimum_order_quantity":"10","supply_region":["CN"],"valid_from":"2026-01-01","valid_to":"2026-12-31"});
        PortalBatchRow {
            row_id: id.into(),
            idempotency_key: key.into(),
            input: json!({"snapshot":{"kind":"EXISTING_QUOTE","sku_id":id,"target_version":{"sku_version":1,"sku_revision_id":"sku-revision","sku_revision_version":1,"product_id":"product","product_version":1,"product_revision_id":"product-revision","product_revision_version":1,"unit_id":"unit","unit_version":1},"supplier_sku_code":code,"terms":terms,"availability_status":"AVAILABLE","available_quantity":null,"availability_reported_at":Instant::from_unix_secs(10)},"reason":"报价"}),
        }
    }
    #[test]
    fn duplicate_trimmed_ordering_codes_fail_even_with_different_skus_and_keys() {
        let mut input =
            batch(vec![quote_row("sku-a", "key-a", " CODE "), quote_row("sku-b", "key-b", "CODE")]);
        input.mode = PortalBatchMode::Quote;
        assert!(parse_row(input.mode, input.phase, &input.rows[0]).is_ok());
        assert_eq!(duplicate_business_units(&input).len(), 2);
        input.rows[1] = quote_row("sku-b", "key-b", "OTHER");
        assert!(duplicate_business_units(&input).is_empty());
    }
    #[test]
    fn duplicate_new_product_ordering_codes_fail_across_product_groups() {
        let dictionary = json!({"raw_name":"原名"});
        let product = |code: &str| json!({"input":{"name":"新品","product_kind":erp_catalog::ProductKind::Physical,"brand":dictionary,"category":dictionary,"image_asset_ids":[],"file_asset_ids":[],"skus":[{"row_id":"sku-row","name":"新品SKU","spec_entries":[],"unit":dictionary,"ordering_code":code,"supply_terms":{},"available_quantity":null,"reported_at":Instant::from_unix_secs(10)}]}});
        let mut input = batch(vec![
            PortalBatchRow {
                row_id: "group-a".into(),
                idempotency_key: "key-a".into(),
                input: product(" CODE "),
            },
            PortalBatchRow {
                row_id: "group-b".into(),
                idempotency_key: "key-b".into(),
                input: product("CODE"),
            },
        ]);
        input.mode = PortalBatchMode::NewProduct;
        input.phase = Some(PortalBatchPhase::Prepare);
        assert!(parse_row(input.mode, input.phase, &input.rows[0]).is_ok());
        assert_eq!(duplicate_business_units(&input).len(), 2);
        input.rows[1].input = product("OTHER");
        assert!(duplicate_business_units(&input).is_empty());
    }
    #[test]
    fn supplier_scope_prevents_cross_binding_replay() {
        let payload = json!({"quantity":"1"});
        let a = scoped_command(
            "same-user",
            "supplier-a",
            "supplier_portal.availability_update",
            "same-key",
            &payload,
        )
        .unwrap();
        let b = scoped_command(
            "same-user",
            "supplier-b",
            "supplier_portal.availability_update",
            "same-key",
            &payload,
        )
        .unwrap();
        assert_ne!(a.id(), b.id());
        assert_eq!(a.scope_id(), Some("supplier-a"));
    }
    #[test]
    fn external_batch_result_strips_internal_fields() {
        let value = external_result(
            json!({"id":"app","version":2,"status":"SUBMITTED","supplier_id":"private","handler_id":"buyer","submissions":["secret"],"normalized_product":{"private":true}}),
        );
        assert_eq!(value, json!({"id":"app","version":2,"status":"SUBMITTED","result":null}));
    }
    #[test]
    fn new_product_batch_result_is_an_editable_draft_without_formal_or_review_identity() {
        let dictionary = json!({"raw_name":"原名"});
        let row = PortalBatchRow {
            row_id: "group-a".into(),
            idempotency_key: "original-key".into(),
            input: json!({"input":{"name":"新品","product_kind":erp_catalog::ProductKind::Physical,"brand":dictionary,"category":dictionary,"image_asset_ids":[],"file_asset_ids":[],"skus":[{"row_id":"sku-row","name":"新品SKU","spec_entries":[],"unit":dictionary,"ordering_code":"CODE","supply_terms":{},"available_quantity":null,"reported_at":Instant::from_unix_secs(10)}]}}),
        };
        let parsed = parse_row(PortalBatchMode::NewProduct, Some(PortalBatchPhase::Prepare), &row).unwrap();
        assert_eq!(batch_action(&parsed), "supplier_portal.new_product_save");
        let PreparedInput::NewProduct(req) = parsed else { panic!("expected new product") };
        req.input.validate_submission().unwrap();
        let mut draft = erp_catalog::portal::NewProductDraft::new(
            "draft-a".into(),
            "supplier-a".into(),
            "external-a".into(),
            req.input,
        )
        .unwrap();
        assert!(draft.current_submission_id.is_none());
        assert!(draft.submissions.is_empty());
        assert!(draft.task_id.is_none());
        assert!(draft.result.is_none());
        let view = external_result(serialize(&draft).unwrap());
        assert_eq!(view, json!({"id":"draft-a","version":1,"status":"draft","result":null}));
        let mut with_image = draft.draft.clone();
        with_image.image_asset_ids.push("uploaded-to-this-draft".into());
        draft.update(draft.base.version, with_image).unwrap();
        assert_eq!(draft.draft.image_asset_ids, vec!["uploaded-to-this-draft"]);
        assert!(draft.task_id.is_none());
        assert!(draft.result.is_none());
    }

    #[test]
    fn new_product_final_batch_uses_original_draft_and_a_distinct_submit_command() {
        let row = PortalBatchRow {
            row_id: "group-a".into(),
            idempotency_key: "original-submit-key".into(),
            input: json!({"id":"draft-a","expected_version":3}),
        };
        let mut input = batch(vec![row.clone()]);
        input.mode = PortalBatchMode::NewProduct;
        assert!(validate_rows(&input).is_err());
        input.phase = Some(PortalBatchPhase::Submit);
        validate_rows(&input).unwrap();
        let parsed = parse_row(input.mode, input.phase, &row).unwrap();
        assert_eq!(batch_action(&parsed), "supplier_portal.new_product_submit");
        let PreparedInput::NewProductSubmit(id, transition) = parsed else {
            panic!("expected final submission")
        };
        assert_eq!(id, "draft-a");
        assert_eq!(transition.expected_version, 3);
        assert_eq!(transition.idempotency_key, "original-submit-key");
        let mut forged = row;
        forged.input["image_asset_ids"] = json!(["other-draft-image"]);
        assert!(parse_row(input.mode, input.phase, &forged).is_err());
        input.mode = PortalBatchMode::Quote;
        assert!(validate_rows(&input).is_err());
    }

    #[test]
    fn actual_sku_counts_and_duplicate_ordering_codes_block_whole_final_batch() {
        let plan = |id: &str, count: usize, code: &str| PreparedRow {
            row: PortalBatchRow {
                row_id: id.into(),
                idempotency_key: format!("key-{id}"),
                input: json!({"id":id,"expected_version":1}),
            },
            input: PreparedInput::NewProductSubmit(
                id.into(),
                PortalTransition { expected_version: 1, idempotency_key: format!("key-{id}") },
            ),
            replay: None,
            rejection: None,
            sku_count: count,
            business_units: vec![format!("request:{id}"), format!("ordering_code:{code}")],
        };
        let results = || vec![row_result("a", "valid", None, None), row_result("b", "valid", None, None)];
        let mut rows = results();
        validate_prepared_rows(&[plan("a", 50, "A"), plan("b", 50, "B")], &mut rows);
        assert!(rows.iter().all(|row| row.status == "valid"));
        validate_prepared_rows(&[plan("a", 50, "A"), plan("b", 51, "B")], &mut rows);
        assert!(rows.iter().all(|row| row.status == "validation_failed"));
        rows = results();
        validate_prepared_rows(&[plan("a", 1, "SAME"), plan("b", 1, "SAME")], &mut rows);
        assert!(rows.iter().all(|row| row.status == "validation_failed"));
    }

    #[test]
    fn mixed_recovery_retains_unknown_until_each_original_command_has_a_terminal_receipt() {
        let mut input = batch(vec![row("a", "key-a")]);
        input.recovery_only = true;
        let rejected = json!({"command_outcome":"rejected"});
        let succeeded = json!({"id":"offering","version":2});
        let mut rows = vec![
            row_result("valid", "valid", None, None),
            row_result("duplicate", "validation_failed", Some("重复单元".into()), None),
            row_result("replayed", "replayed", None, Some(succeeded.clone())),
            row_result("sealed", "failed", Some("旧版本已封存".into()), Some(rejected.clone())),
            row_result("uncertain", "unknown", Some("提交结果未知".into()), None),
        ];
        retain_unresolved_recovery(&input, false, &mut rows);
        assert_eq!(
            rows.iter().map(|row| row.status.as_str()).collect::<Vec<_>>(),
            vec!["unknown", "unknown", "replayed", "failed", "unknown"]
        );
        assert_eq!(rows[2].result, Some(succeeded));
        assert_eq!(rows[3].result, Some(rejected));
        assert!(rows[0].result.is_none());
        assert_eq!(rows[1].error.as_deref(), Some("重复单元"));
        assert!(!batch_rows_valid(&rows));
    }

    #[test]
    fn recovery_validate_only_does_not_release_the_original_command_without_a_receipt() {
        let mut input = batch(vec![row("a", "key-a")]);
        input.recovery_only = true;
        input.validate_only = true;
        let mut rows = vec![row_result("a", "valid", None, None)];
        retain_unresolved_recovery(&input, true, &mut rows);
        assert_eq!(rows[0].status, "unknown");
        assert!(!batch_rows_valid(&rows));
        input.recovery_only = false;
        rows[0].status = "validation_failed".into();
        retain_unresolved_recovery(&input, false, &mut rows);
        assert_eq!(rows[0].status, "validation_failed");
    }

    #[test]
    fn aggregate_preflight_cannot_replace_a_sealed_rejection_with_validation_failed() {
        let prepared = vec![PreparedRow {
            row: row("a", "key-a"),
            input: parse_row(PortalBatchMode::Availability, None, &row("a", "key-a")).unwrap(),
            replay: None,
            rejection: None,
            sku_count: 101,
            business_units: Vec::new(),
        }];
        let marker = json!({"command_outcome":"rejected"});
        let mut rows = vec![row_result("sealed", "failed", Some("版本已封存".into()), Some(marker.clone()))];
        validate_prepared_rows(&prepared, &mut rows);
        assert_eq!(rows[0].status, "failed");
        assert_eq!(rows[0].result, Some(marker));
    }

    #[test]
    fn qualification_configuration_failures_remain_authorization_errors() {
        for error in [
            Error::ValidationError("供给范围不支持仓库维度".into()),
            Error::BusinessLogicError("责任组织配置无效".into()),
            Error::ConflictError("DATA_SCOPE_CHANGED".into()),
        ] {
            assert!(matches!(qualification_error(error), Error::Rbac(_)));
        }
        assert!(matches!(qualification_error(Error::Forbidden("没有范围".into())), Error::Forbidden(_)));
        assert!(matches!(qualification_error(Error::Internal("读取失败".into())), Error::Internal(_)));
    }

    #[test]
    fn recovery_execution_rejections_cannot_release_an_earlier_unknown_command() {
        let errors = || {
            [
                Error::Forbidden("资格在预检后失效".into()),
                Error::Unauthenticated("账号在预检后停用".into()),
                Error::ValidationError("执行前配置变化".into()),
                Error::ConflictError("执行前版本变化".into()),
                Error::Rbac("权限配置无法证明".into()),
            ]
        };
        for error in errors() {
            let result = execution_row_result("original-row", true, Err(error));
            assert_eq!(result.status, "unknown");
            assert_eq!(result.row_id, "original-row");
            assert!(result.error.is_some());
            assert!(result.result.is_none());
        }
        for error in errors() {
            assert_eq!(execution_row_result("new-row", false, Err(error)).status, "failed");
        }
    }

    #[test]
    fn confirmed_execution_success_and_uncertain_commit_keep_their_result_classifications() {
        for recovering in [false, true] {
            let success = execution_row_result(
                "row",
                recovering,
                Ok(json!({"id":"original-application","version":3,"status":"pending"})),
            );
            assert_eq!(success.status, "succeeded");
            assert_eq!(success.result.unwrap()["id"], "original-application");
            let uncertain = execution_row_result(
                "row",
                recovering,
                Err(Error::ReceiptDuplicate(persistence_core::Error::OptimisticLockingError)),
            );
            assert_eq!(uncertain.status, "unknown");
            assert!(uncertain.result.is_none());
        }
    }
}
