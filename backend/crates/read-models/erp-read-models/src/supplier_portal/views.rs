//! 三类申请的外部字段允许列表，内部建档映射与任务配置不进入输出。

use erp_catalog::portal::{DraftStatus, NewProductDraft};
use erp_supplier::portal::{CooperationApplication, CooperationStatus};
use erp_supply::portal::{ApplicationKind, ApplicationStatus, OfferingApplication};
use serde::Serialize;
use serde_json::Value;

use crate::{Error, Result};

/// 外部可追溯提交，不包含内部处理人、组织、任务或任务版本。
#[derive(Debug, Serialize)]
pub struct PortalSubmissionView {
    pub id: String,
    pub submission_no: Option<u32>,
    pub submitted_by: String,
    pub submitted_at: Value,
    pub input: Value,
    pub snapshot: Value,
}

/// 供应商可见决定，商品建档的内部规范化快照单独留在内部。
#[derive(Debug, Serialize)]
pub struct PortalDecisionView {
    pub submission_id: String,
    pub status: String,
    pub reason: Option<String>,
    pub decided_by: String,
    pub decided_at: Value,
    pub result: Option<Value>,
}

/// 与正式业务事实分开显示的统一门户申请。
#[derive(Debug, Serialize)]
pub struct PortalApplicationView {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub version: u64,
    pub input: Value,
    pub snapshot: Value,
    pub reason: String,
    pub current_submission_id: Option<String>,
    pub submissions: Vec<PortalSubmissionView>,
    pub decisions: Vec<PortalDecisionView>,
    pub result: Option<Value>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl PortalApplicationView {
    /// 装配本供应商报价或供给修订申请。
    ///
    /// # 参数
    /// `app` 为已完成供应商归属校验的正式申请事实。
    /// # 返回
    /// 供应商原稿及允许的历史，不包含任务配置。
    /// # 错误
    /// 领域快照序列化失败时返回错误。
    pub fn from_offering(app: &OfferingApplication) -> Result<Self> {
        let input = value(&app.snapshot)?;
        let submissions = app
            .submissions
            .iter()
            .map(|submission| {
                let input = value(&submission.snapshot)?;
                Ok(PortalSubmissionView {
                    id: submission.submission_no.to_string(),
                    submission_no: Some(submission.submission_no),
                    submitted_by: submission.submitted_by.clone(),
                    submitted_at: value(submission.submitted_at)?,
                    snapshot: input.clone(),
                    input,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let decisions = app
            .decisions
            .iter()
            .map(|decision| {
                Ok(PortalDecisionView {
                    submission_id: decision.submission_no.to_string(),
                    status: offering_status(decision.status).into(),
                    reason: Some(decision.reason.clone()),
                    decided_by: decision.decided_by.clone(),
                    decided_at: value(decision.decided_at)?,
                    result: None,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            id: app.base.id.clone(),
            kind: offering_kind(app.kind).into(),
            status: offering_status(app.status).into(),
            version: app.base.version,
            snapshot: input.clone(),
            input,
            reason: app.reason.clone(),
            current_submission_id: app
                .submissions
                .last()
                .map(|submission| submission.submission_no.to_string()),
            submissions,
            decisions,
            result: app.result.as_ref().map(value).transpose()?,
            created_at: app.base.created_at,
            updated_at: app.base.updated_at,
        })
    }

    /// 装配新品原始提报及实际建档结果。
    ///
    /// # 参数
    /// `app` 为当前供应商可见的新品暂存。
    /// # 返回
    /// 保留原始字典资料及所有 SKU，剥离内部规范化和匹配配置。
    /// # 错误
    /// 原稿或结果序列化失败时返回错误。
    pub fn from_new_product(app: &NewProductDraft) -> Result<Self> {
        let input = value(&app.draft)?;
        let submissions = app
            .submissions
            .iter()
            .map(|submission| {
                let input = value(&submission.input)?;
                Ok(PortalSubmissionView {
                    id: submission.id.clone(),
                    submission_no: None,
                    submitted_by: submission.submitted_by.clone(),
                    submitted_at: value(submission.submitted_at)?,
                    snapshot: input.clone(),
                    input,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let decisions = app
            .submissions
            .iter()
            .filter_map(|submission| submission.decision.as_ref().map(|decision| (submission, decision)))
            .map(|(submission, decision)| {
                Ok(PortalDecisionView {
                    submission_id: submission.id.clone(),
                    status: draft_status(decision.status).into(),
                    reason: decision.reason.clone(),
                    decided_by: decision.decided_by.clone(),
                    decided_at: value(decision.decided_at)?,
                    result: decision.result.as_ref().map(value).transpose()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            id: app.base.id.clone(),
            kind: "NEW_PRODUCT".into(),
            status: draft_status(app.status).into(),
            version: app.base.version,
            input,
            snapshot: app.frozen_input.as_ref().map(value).transpose()?.unwrap_or(value(&app.draft)?),
            reason: String::new(),
            current_submission_id: app.current_submission_id.clone(),
            submissions,
            decisions,
            result: app.result.as_ref().map(value).transpose()?,
            created_at: app.base.created_at,
            updated_at: app.base.updated_at,
        })
    }

    /// 装配供应商商务档案合作条款申请。
    ///
    /// # 参数
    /// `app` 为当前供应商已授权申请。
    /// # 返回
    /// 仅付款条件原稿、历次提交和决定，剥离采购处理人与任务标识。
    /// # 错误
    /// 快照或结果序列化失败时返回错误。
    pub fn from_cooperation(app: &CooperationApplication) -> Result<Self> {
        let input = value(&app.proposal)?;
        let submissions = app
            .submissions
            .iter()
            .map(|submission| {
                let input = value(&submission.proposal)?;
                Ok(PortalSubmissionView {
                    id: submission.submission_no.to_string(),
                    submission_no: Some(submission.submission_no),
                    submitted_by: submission.submitted_by.clone(),
                    submitted_at: value(submission.submitted_at)?,
                    snapshot: input.clone(),
                    input,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let decisions = app
            .decisions
            .iter()
            .map(|decision| {
                Ok(PortalDecisionView {
                    submission_id: decision.submission_no.to_string(),
                    status: cooperation_status(decision.status).into(),
                    reason: decision.reason.clone(),
                    decided_by: decision.actor_id.clone(),
                    decided_at: value(decision.decided_at)?,
                    result: None,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            id: app.base.id.clone(),
            kind: "COOPERATION".into(),
            status: cooperation_status(app.status).into(),
            version: app.base.version,
            snapshot: input.clone(),
            input,
            reason: app.proposal.reason.clone(),
            current_submission_id: app
                .submissions
                .last()
                .map(|submission| submission.submission_no.to_string()),
            submissions,
            decisions,
            result: app.result.as_ref().map(value).transpose()?,
            created_at: app.base.created_at,
            updated_at: app.base.updated_at,
        })
    }
}

/// 只将已声明允许的值送入输出；不序列化整个领域根对象。
///
/// # 参数
/// * `value` - 已声明允许进入输出的值。
///
/// # 返回
/// 返回对应的 JSON 值。
///
/// # 错误
/// 序列化失败时返回 `Internal`。
pub(super) fn value<T: Serialize>(value: T) -> Result<Value> {
    serde_json::to_value(value).map_err(|error| Error::Internal(error.to_string()))
}

/// 统一外部报价业务名称。
fn offering_kind(kind: ApplicationKind) -> &'static str {
    match kind {
        ApplicationKind::ExistingQuote => "EXISTING_QUOTE",
        ApplicationKind::TermsChange => "TERMS_CHANGE",
        ApplicationKind::StopSupply => "STOP_SUPPLY",
    }
}
/// 统一供给申请的展示状态。
fn offering_status(status: ApplicationStatus) -> &'static str {
    match status {
        ApplicationStatus::Draft => "DRAFT",
        ApplicationStatus::Submitted => "SUBMITTED",
        ApplicationStatus::Returned => "RETURNED",
        ApplicationStatus::Withdrawn => "WITHDRAWN",
        ApplicationStatus::Effective => "EFFECTIVE",
    }
}
/// 新品待审核与单人确认待处理采用同一对外状态。
fn draft_status(status: DraftStatus) -> &'static str {
    match status {
        DraftStatus::Draft => "DRAFT",
        DraftStatus::Pending => "SUBMITTED",
        DraftStatus::Returned => "RETURNED",
        DraftStatus::Withdrawn => "WITHDRAWN",
        DraftStatus::Effective => "EFFECTIVE",
    }
}
/// 合作申请与供给申请统一展示状态。
fn cooperation_status(status: CooperationStatus) -> &'static str {
    match status {
        CooperationStatus::Draft => "DRAFT",
        CooperationStatus::Submitted => "SUBMITTED",
        CooperationStatus::Returned => "RETURNED",
        CooperationStatus::Withdrawn => "WITHDRAWN",
        CooperationStatus::Effective => "EFFECTIVE",
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use entity_core::BaseModel;
    use erp_catalog::portal::NewProductDraft;
    use erp_core::AccountKind;
    use erp_supplier::portal::{CooperationApplication, CooperationRequest};
    use erp_supplier::{ReconciliationCycle, SettlementMode};

    use super::PortalApplicationView;

    #[test]
    fn cooperation_history_projection_keeps_supplier_input_and_excludes_task_configuration() {
        let actor = AuditActor::new("external1".into(), "supplier01".into(), AccountKind::Supplier);
        let request = CooperationRequest {
            expected_supplier_version: 3,
            expected_profile_id: "profile1".into(),
            settlement_mode: SettlementMode::Prepayment,
            reconciliation_cycle: ReconciliationCycle::None,
            payment_term: "PREPAY_100".into(),
            reason: "申请调整".into(),
        };
        let mut app = CooperationApplication::new("app1".into(), "supplier1", request, &actor).unwrap();
        app.submit("supplier1", 1, "internal-buyer", "internal-task", &actor, 12).unwrap();
        let public = serde_json::to_value(PortalApplicationView::from_cooperation(&app).unwrap()).unwrap();
        assert_eq!(public["kind"], "COOPERATION");
        assert_eq!(public["status"], "SUBMITTED");
        assert_eq!(public["input"]["payment_term"], "PREPAY_100");
        assert_eq!(public["submissions"][0]["submitted_by"], "external1");
        assert!(public["submissions"][0].get("procurement_owner_id").is_none());
        assert!(public["submissions"][0].get("task_id").is_none());
    }

    #[test]
    fn new_product_projection_preserves_raw_input_and_strips_normalized_mapping() {
        let input = serde_json::json!({
            "name":"供应商原始名称","product_kind":"PHYSICAL",
            "brand":{"raw_name":"品牌建议","selected_id":null,"expected_version":null},
            "category":{"raw_name":"礼品 / 杯具","selected_id":null,"expected_version":null},
            "model":null,"description":null,"image_asset_ids":[],"file_asset_ids":[],"skus":[]
        });
        let mut record = serde_json::to_value(BaseModel::fake()).unwrap();
        let root = record.as_object_mut().unwrap();
        root.insert("supplier_id".into(), "supplier1".into());
        root.insert("created_by".into(), "external1".into());
        root.insert("draft".into(), input.clone());
        root.insert("status".into(), "returned".into());
        root.insert("frozen_input".into(), input.clone());
        root.insert("current_submission_id".into(), "submission1".into());
        root.insert("task_id".into(), serde_json::Value::Null);
        root.insert("result".into(), serde_json::Value::Null);
        root.insert("submissions".into(),serde_json::json!([{
            "id":"submission1","input":input,"submitted_by":"external1","submitted_at":1_791_072_000,"task_id":"internal-task",
            "decision":{"status":"returned","reason":"请核对单位","decided_by":"buyer1","decided_at":1_791_075_600,
                "normalized_product":{"name":"供应商原始名称","brand_id":"internal-brand","brand_version":1,"category_id":"internal-category","category_version":1,
                    "category_hierarchy":[
                        {"id":"internal-root","version":2,"name":"礼品","parent_id":null,"product_kind":"PHYSICAL"},
                        {"id":"internal-category","version":1,"name":"杯具","parent_id":"internal-root","product_kind":"PHYSICAL"}
                    ],"sku_mappings":[]},"result":null}
        }]));
        let app: NewProductDraft = serde_json::from_value(record).unwrap();
        let public = serde_json::to_value(PortalApplicationView::from_new_product(&app).unwrap()).unwrap();
        assert_eq!(public["input"]["brand"]["raw_name"], "品牌建议");
        assert_eq!(public["decisions"][0]["reason"], "请核对单位");
        assert!(public["decisions"][0].get("normalized_product").is_none());
        assert!(public["submissions"][0].get("task_id").is_none());
    }
}
