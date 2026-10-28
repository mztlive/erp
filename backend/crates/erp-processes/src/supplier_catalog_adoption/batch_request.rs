//! 批次 HTTP 合同；用户提交草稿及明确确认范围，权威金额和目标由服务器产生。

use std::collections::HashSet;

use erp_supply::catalog_adoption::{
    AdoptionMapping, BatchConfirmation, BatchCounts, BatchDefaults, BatchPricing, BatchPricingDraft,
    BatchSalesEdit, BatchValuePatch, CatalogAdoptionBatch, CatalogAdoptionBatchItem, FillPolicy,
    adoption_identity,
};
use erp_supply::dto::supplier_catalog::CatalogFilter;
use erp_supply::dto::supplier_offering::SupplierOfferingTermsWrite;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// 在点击全筛选选择或配置入库时冻结来源成员。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateBatchRequest {
    pub item_ids: Option<Vec<String>>,
    pub filter: Option<CatalogFilter>,
    #[serde(default)]
    pub excluded_ids: Vec<String>,
}

/// 单项调整不接收目标 SKU 或字段来源标记。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchItemEdit {
    pub item_id: String,
    pub mapping: Option<AdoptionMapping>,
    pub terms: Option<SupplierOfferingTermsWrite>,
    pub sales_edit: Option<BatchSalesEdit>,
    #[serde(default)]
    pub delivery_template_id: BatchValuePatch<String>,
    #[serde(default)]
    pub restore_pricing: bool,
    pub warnings_confirmed: Option<bool>,
}

impl BatchItemEdit {
    /// 区分会改变已核对载荷的编辑与单纯确认价格提示。
    /// # 参数
    /// 无。
    /// # 返回
    /// 改价、资料、配送或恢复规则时返回真。
    /// # 错误
    /// 不返回错误。
    pub(super) fn changes_content(&self) -> bool {
        self.mapping.is_some()
            || self.terms.is_some()
            || self.sales_edit.is_some()
            || !matches!(self.delivery_template_id, BatchValuePatch::Keep)
            || self.restore_pricing
    }
}

/// 已保存服务端草稿的局部更新；缺失字段保持原值。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveBatchRequest {
    pub expected_version: u64,
    pub rules: Option<BatchPricing>,
    pub inactive_pricing: Option<BatchPricingDraft>,
    pub fill_policy: Option<FillPolicy>,
    pub defaults: Option<BatchDefaults>,
    pub step: Option<String>,
    pub excluded_ids: Option<Vec<String>>,
    #[serde(default)]
    pub items: Vec<BatchItemEdit>,
}

/// 当前页面最后读取到的服务端版本。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchVersionRequest {
    pub expected_version: u64,
    pub item_ids: Option<Vec<String>>,
}

/// 明确确认就绪子集，固定本次操作号和最后核对版本。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitBatchRequest {
    pub expected_version: u64,
    pub preview_version: u64,
    pub item_ids: Vec<String>,
    pub idempotency_key: String,
}

impl SubmitBatchRequest {
    /// 生成原确认未受理时的终止标记，阻止同操作号的迟到请求再次执行。
    /// # 参数
    /// `batch` 为当前已授权批次。
    /// # 返回
    /// 与正常确认摘要区分的稳定标记，不依赖之后可能修改的逐项草稿。
    /// # 错误
    /// 操作号或确认范围无效时拒绝。
    pub(super) fn unaccepted_fingerprint(&self, batch: &str) -> Result<String> {
        let unique: HashSet<_> = self.item_ids.iter().collect();
        if self.idempotency_key.trim().is_empty()
            || self.idempotency_key.len() > 128
            || self.item_ids.is_empty()
            || unique.len() != self.item_ids.len()
            || self.item_ids.iter().any(|id| id.trim().is_empty())
        {
            return Err(Error::ValidationError("原入库确认身份或范围无效".into()));
        }
        let payload = serde_json::to_string(&(batch, self.preview_version, &self.item_ids))
            .map_err(|_| Error::Internal("原入库确认无法编码".into()))?;
        Ok(format!("NOT_ACCEPTED:{}", adoption_identity(&[&payload])))
    }

    /// 核验查证请求精确对应原确认人、预览与成员范围。
    /// # 参数
    /// `confirmation` 为永久确认记录，`actor` 为当前认证账号。
    /// # 返回
    /// 原请求与记录一致时成功。
    /// # 错误
    /// 操作身份或确认范围不一致时拒绝，不能拿其他操作解除待核实。
    pub(super) fn ensure_confirmation(&self, confirmation: &BatchConfirmation, actor: &str) -> Result<()> {
        if confirmation.actor_id != actor
            || confirmation.preview_version != self.preview_version
            || confirmation.item_ids != self.item_ids
        {
            return Err(Error::ConflictError("原入库确认与查询范围不一致".into()));
        }
        Ok(())
    }
}

/// 原确认是否已经受理；未受理结果已持久化阻止迟到请求。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BatchSubmissionStatus {
    Accepted,
    NotAccepted,
}

/// 原确认查证结果与当前批次，不能以逐行状态推断另一操作是否受理。
#[derive(Debug, Serialize)]
pub struct BatchSubmissionView {
    pub status: BatchSubmissionStatus,
    pub view: BatchView,
}

/// 完整冻结范围及互斥统计，刷新后直接恢复服务端真实状态。
#[derive(Debug, Serialize)]
pub struct BatchView {
    pub batch: CatalogAdoptionBatch,
    #[serde(serialize_with = "serialize_batch_rows")]
    pub rows: Vec<CatalogAdoptionBatchItem>,
    pub counts: BatchCounts,
    pub product_count: usize,
    pub manual_count: usize,
}

/// 在响应边界附带与自动匹配一致的单位建议，不改写持久化来源快照。
fn serialize_batch_rows<S: serde::Serializer>(
    rows: &[CatalogAdoptionBatchItem],
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    #[derive(Serialize)]
    struct DisplayRow<'a> {
        #[serde(flatten)]
        row: &'a CatalogAdoptionBatchItem,
        suggested_unit: Option<String>,
        unit_inferred: bool,
    }
    rows.iter()
        .map(|row| {
            let suggested_unit = row.source.suggested_unit();
            let unit_inferred = suggested_unit.is_some()
                && row.source.unit.as_deref().is_none_or(|unit| unit.trim().is_empty());
            DisplayRow { row, suggested_unit, unit_inferred }
        })
        .collect::<Vec<_>>()
        .serialize(serializer)
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use serde_json::json;

    use super::*;

    #[test]
    fn pricing_step_accepts_browser_payload_with_unfilled_details() {
        let request: SaveBatchRequest = serde_json::from_str(
            r#"{
            "expected_version":1,
            "rules":{
                "mode":"SEPARATE",
                "dropship":{"method":"FIXED","value":"200"},
                "bulk":{"method":"FIXED","value":"200"},
                "minimum_quantity":"100"
            },
            "inactive_pricing":{"mode":"SAME","rule":{"method":"UNSET"},"minimum_quantity":null},
            "fill_policy":{"overwrite":true,"preserve_manual":true},
            "defaults":{
                "category_id":null,"category_mappings":[],"input_tax_rate":null,
                "valid_from":null,"valid_to":null,"delivery_template_id":null,"clear_fields":[]
            },
            "step":"DETAILS",
            "items":[]
        }"#,
        )
        .unwrap();
        assert_eq!(request.step.as_deref(), Some("DETAILS"));
        let BatchPricing::Separate { dropship, bulk, minimum_quantity } = request.rules.unwrap() else {
            panic!("separate pricing must retain both rules");
        };
        assert_eq!(dropship.calculate(None).unwrap().unwrap().to_string(), "200");
        assert_eq!(bulk.calculate(None).unwrap().unwrap().to_string(), "200");
        assert_eq!(minimum_quantity.unwrap().to_string(), "100");
        assert!(request.defaults.unwrap().category_mappings.is_empty());
    }

    #[test]
    fn pricing_step_preserves_unfilled_inactive_rules_without_applying_them() {
        let inactive = json!({
            "mode":"SEPARATE",
            "dropship":{"method":"FIXED","value":"200"},
            "bulk":{"method":"PERCENTAGE","value":""},
            "minimum_quantity":""
        });
        let request: SaveBatchRequest = serde_json::from_value(json!({
            "expected_version":1,
            "rules":{"mode":"SAME","rule":{"method":"FIXED","value":"200"},"minimum_quantity":"100"},
            "inactive_pricing":inactive,
            "step":"DETAILS"
        }))
        .unwrap();
        assert_eq!(serde_json::to_value(request.inactive_pricing.unwrap()).unwrap(), inactive);
        let BatchPricing::Same { rule, minimum_quantity } = request.rules.unwrap() else {
            panic!("only the active same-price rule may apply");
        };
        assert_eq!(rule.calculate(None).unwrap().unwrap().to_string(), "200");
        assert_eq!(minimum_quantity.unwrap().to_string(), "100");
    }

    #[test]
    fn unaccepted_identity_is_stable_and_distinct_from_a_normal_command() {
        let mut request = SubmitBatchRequest {
            expected_version: 10,
            preview_version: 8,
            item_ids: vec!["first".into(), "second".into()],
            idempotency_key: "original".into(),
        };
        let original = request.unaccepted_fingerprint("batch").unwrap();
        assert!(original.starts_with("NOT_ACCEPTED:"));
        request.expected_version = 20;
        assert_eq!(request.unaccepted_fingerprint("batch").unwrap(), original);
        assert_ne!(request.unaccepted_fingerprint("other-batch").unwrap(), original);
        request.preview_version = 9;
        assert_ne!(request.unaccepted_fingerprint("batch").unwrap(), original);
        request.item_ids.push("first".into());
        assert!(matches!(request.unaccepted_fingerprint("batch"), Err(Error::ValidationError(_))));
        request.item_ids.clear();
        assert!(request.unaccepted_fingerprint("batch").is_err());
        request.item_ids.push("first".into());
        request.idempotency_key.clear();
        assert!(request.unaccepted_fingerprint("batch").is_err());
    }

    #[test]
    fn submission_query_requires_original_actor_preview_and_scope() {
        let request = SubmitBatchRequest {
            expected_version: 10,
            preview_version: 8,
            item_ids: vec!["first".into()],
            idempotency_key: "original".into(),
        };
        let mut confirmation = BatchConfirmation {
            operation_id: "command".into(),
            actor_id: "actor".into(),
            confirmed_at: Instant::now(),
            preview_version: 8,
            item_ids: vec!["first".into()],
            fingerprint: "payload".into(),
        };
        request.ensure_confirmation(&confirmation, "actor").unwrap();
        assert!(request.ensure_confirmation(&confirmation, "other").is_err());
        confirmation.preview_version += 1;
        assert!(request.ensure_confirmation(&confirmation, "actor").is_err());
        confirmation.preview_version = 8;
        confirmation.item_ids = vec!["other-item".into()];
        assert!(request.ensure_confirmation(&confirmation, "actor").is_err());
    }

    #[test]
    fn warning_acknowledgement_keeps_review_but_explicit_edits_invalidate_it() {
        let acknowledgement: BatchItemEdit = serde_json::from_value(json!({
            "item_id":"source", "warnings_confirmed":true
        }))
        .unwrap();
        assert!(!acknowledgement.changes_content());
        for patch in [
            json!({"item_id":"source","sales_edit":{"dropship":{"action":"SET","value":"118.00"}}}),
            json!({"item_id":"source","sales_edit":{"market_price":{"action":"CLEAR"}}}),
            json!({"item_id":"source","delivery_template_id":{"action":"CLEAR"}}),
            json!({"item_id":"source","restore_pricing":true}),
        ] {
            let edit: BatchItemEdit = serde_json::from_value(patch).unwrap();
            assert!(edit.changes_content());
        }
    }
}
