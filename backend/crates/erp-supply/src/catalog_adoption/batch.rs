//! 持久化批次、逐项草稿和执行状态；未知结果只能查证原操作。

use std::collections::{BTreeMap, HashSet};

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};

use super::{
    AdoptionMapping, AdoptionResult, AdoptionRow, BatchMatchAnchor, BatchPricing, BatchSalesValues,
    FillPolicy, ValueOrigin,
};
use crate::dto::supplier_catalog::CatalogFilter;
use crate::dto::supplier_offering::SupplierOfferingTermsWrite;
use crate::entity::supplier_catalog::SupplierCatalogItem;
use crate::{Error, Result};

/// 单个来源分类对应的公司分类，不自动创建字典。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchCategoryMapping {
    /// 空值明确对应来源未分类组，不作为其他来源分类的默认值。
    pub source_category_id: Option<String>,
    pub category_id: String,
}

/// 批量资料补值；清空、人工值保护另由逐项来源标记决定。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchDefaults {
    pub category_id: Option<String>,
    #[serde(default)]
    pub category_mappings: Vec<BatchCategoryMapping>,
    pub input_tax_rate: Option<String>,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub delivery_template_id: Option<String>,
    #[serde(default)]
    pub clear_fields: Vec<String>,
}

impl BatchDefaults {
    /// 核验逐来源分类映射不重复，允许明确映射来源未分类组。
    /// # 参数
    /// 无。
    /// # 返回
    /// 映射结构合法时成功。
    /// # 错误
    /// 空白分类身份、空公司分类或同一来源分类重复映射时拒绝。
    pub fn validate(&self) -> Result<()> {
        self.validate_clears()?;
        let mut seen = HashSet::new();
        for mapping in &self.category_mappings {
            if mapping.source_category_id.as_deref().is_some_and(|id| id.trim().is_empty())
                || mapping.category_id.trim().is_empty()
                || !seen.insert(&mapping.source_category_id)
            {
                return Err(Error::ValidationError("来源分类映射不能为空或重复".into()));
            }
        }
        Ok(())
    }

    fn validate_clears(&self) -> Result<()> {
        let mut seen = HashSet::new();
        for field in &self.clear_fields {
            let populated = match field.as_str() {
                "category_id" => self.category_id.is_some() || !self.category_mappings.is_empty(),
                "input_tax_rate" => self.input_tax_rate.is_some(),
                "valid_from" => self.valid_from.is_some(),
                "valid_to" => self.valid_to.is_some(),
                "delivery_template_id" => self.delivery_template_id.is_some(),
                _ => return Err(Error::ValidationError("不允许批量清空该字段".into())),
            };
            if populated || !seen.insert(field) {
                return Err(Error::ValidationError("同一字段不能重复清空或同时补值".into()));
            }
        }
        Ok(())
    }

    /// 优先采用逐来源分类映射，再采用统一公司分类。
    /// # 参数
    /// `item` 为冻结的来源 SKU。
    /// # 返回
    /// 此项适用的公司分类；没有配置时为空。
    /// # 错误
    /// 不返回错误。
    pub fn category_for(&self, item: &SupplierCatalogItem) -> Option<&str> {
        self.category_mappings
            .iter()
            .find(|mapping| item.category_id.as_deref() == mapping.source_category_id.as_deref())
            .map(|mapping| mapping.category_id.as_str())
            .or(self.category_id.as_deref())
    }
}

/// 一次明确确认的不可变范围、操作人及载荷摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchConfirmation {
    pub operation_id: String,
    pub actor_id: String,
    pub confirmed_at: Instant,
    pub preview_version: u64,
    pub item_ids: Vec<String>,
    pub fingerprint: String,
}

/// 作业批次保存选择范围和规则；来源完整资料分行保存，避免大文档限制。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct CatalogAdoptionBatch {
    #[serde(flatten)]
    pub base: BaseModel,
    pub batch_no: String,
    pub connection_id: String,
    pub supplier_id: String,
    pub actor_id: String,
    pub selection_filter: Option<CatalogFilter>,
    pub selected_count: usize,
    pub excluded_ids: Vec<String>,
    pub rules: BatchPricing,
    pub fill_policy: FillPolicy,
    pub defaults: BatchDefaults,
    pub step: String,
    pub preview_version: Option<u64>,
    pub submitted: bool,
    #[serde(default)]
    pub selection_complete: bool,
    #[serde(default)]
    pub execution_pending: bool,
    #[serde(default)]
    pub confirmations: Vec<BatchConfirmation>,
    #[serde(default)]
    pub draft_operation: Option<String>,
    #[serde(default)]
    pub draft_started_at: Option<i64>,
    #[serde(default)]
    pub inactive_pricing: Option<BatchPricing>,
    #[serde(default)]
    pub published_operations: Vec<String>,
}

impl CatalogAdoptionBatch {
    /// 核验没有仍在有效处理时间内的草稿操作。
    /// # 参数
    /// `now` 为服务器当前 Unix 秒数。
    /// # 返回
    /// 未被占用、缺少起始时间或已达到三百秒时允许接管。
    /// # 错误
    /// 草稿操作仍有效或起始时间位于未来时拒绝。
    pub fn ensure_draft_available(&self, now: i64) -> Result<()> {
        if self.draft_operation.is_some()
            && self.draft_started_at.is_some_and(|started| now.saturating_sub(started) < 300)
        {
            return Err(Error::ConflictError("批次正在保存或核对，请稍后重试".into()));
        }
        Ok(())
    }

    /// 核验当前草稿操作仍由原始操作号持有。
    /// # 参数
    /// `token` 为开始草稿操作时保存的完整操作号。
    /// # 返回
    /// 操作号精确一致时成功。
    /// # 错误
    /// 已释放或已被其他操作接管时拒绝。
    pub fn ensure_draft_operation(&self, token: &str) -> Result<()> {
        if self.draft_operation.as_deref() != Some(token) {
            return Err(Error::ConflictError("批次草稿操作已变化，请重新读取后重试".into()));
        }
        Ok(())
    }

    /// 校验客户端读取的版本，禁止覆盖其他页面已保存的草稿。
    /// # 参数
    /// `expected_version` 为最后一次读取的批次版本。
    /// # 返回
    /// 版本仍一致时成功。
    /// # 错误
    /// 版本过期或批次已删除时拒绝。
    pub fn ensure_version(&self, expected_version: u64) -> Result<()> {
        if self.base.is_deleted() || self.base.version != expected_version {
            return Err(Error::ConflictError("入库批次已变化，请刷新后重新核对".into()));
        }
        Ok(())
    }

    /// 修改草稿后取消最终预览资格。
    /// # 参数
    /// 无。
    /// # 返回
    /// 原地清除预览版本。
    /// # 错误
    /// 不返回错误。
    pub fn invalidate_preview(&mut self) {
        self.preview_version = None;
    }

    /// 提交前要求已重新预览当前保存版本且本次确认非空。
    /// # 参数
    /// `preview_version` 为页面最后核对的版本，`item_ids` 为明确确认范围。
    /// # 返回
    /// 当前预览及所选集合有效时成功。
    /// # 错误
    /// 预览失效、零项或重复确认行时拒绝。
    pub fn ensure_submission(&self, preview_version: u64, item_ids: &[String]) -> Result<()> {
        if !self.selection_complete
            || self.draft_operation.is_some()
            || self.preview_version != Some(preview_version)
        {
            return Err(Error::ConflictError("预览已失效，请重新核对后确认入库".into()));
        }
        let unique: HashSet<_> = item_ids.iter().collect();
        if item_ids.is_empty() || unique.len() != item_ids.len() {
            return Err(Error::ValidationError("请确认至少一个不重复的可入库规格".into()));
        }
        Ok(())
    }
}

/// 批次行状态互斥；排除不属于失败，待核实不能直接重试。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BatchItemStatus {
    #[default]
    Draft,
    Ready,
    Processing,
    Succeeded,
    Failed,
    Unknown,
    Skipped,
    Excluded,
}

/// 服务器自动匹配结果；不允许客户端指定目标 SKU。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResolution {
    pub kind: String,
    pub expected_match: String,
    pub sku_no: Option<String>,
    pub product_id: Option<String>,
    pub reason: String,
    #[serde(default)]
    pub group_anchor: Option<BatchMatchAnchor>,
}

/// 来源 SKU 为选品、草稿、确认与恢复的独立单位。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct CatalogAdoptionBatchItem {
    #[serde(flatten)]
    pub base: BaseModel,
    pub batch_id: String,
    pub source: SupplierCatalogItem,
    #[serde(default)]
    pub previous_source: Option<SupplierCatalogItem>,
    pub initially_adopted: bool,
    pub mapping: AdoptionMapping,
    pub terms: Option<SupplierOfferingTermsWrite>,
    pub sales_prices: BatchSalesValues,
    pub delivery_template_id: Option<String>,
    #[serde(default)]
    pub field_origins: BTreeMap<String, ValueOrigin>,
    pub status: BatchItemStatus,
    pub messages: Vec<String>,
    pub warnings: Vec<String>,
    pub warnings_confirmed: bool,
    pub resolution: Option<BatchResolution>,
    pub frozen_row: Option<AdoptionRow>,
    pub result: Option<AdoptionResult>,
    pub operation_id: Option<String>,
    #[serde(default)]
    pub submitted_by: Option<String>,
    pub processing_started_at: Option<i64>,
    pub preview_version: Option<u64>,
}

impl CatalogAdoptionBatchItem {
    /// 保存单项公司映射并区分人工填写、明确清空及采用来源。
    /// # 参数
    /// `mapping` 为完整单项映射；调用方负责使该项及批次的旧预览失效。
    /// # 返回
    /// 保存映射和各字段值来源。
    /// # 错误
    /// 已确认执行、已完成或结果待核实时拒绝。
    pub fn edit_mapping(&mut self, mapping: AdoptionMapping) -> Result<()> {
        if !self.editable() {
            return Err(Error::ConflictError("该项已确认执行或结果待核实，不能调整资料".into()));
        }
        for (field, value) in [("category_id", &mapping.category_id), ("unit_id", &mapping.unit_id)] {
            let origin = if value.trim().is_empty() { ValueOrigin::Cleared } else { ValueOrigin::Manual };
            self.field_origins.insert(field.into(), origin);
        }
        for (field, value, source) in [
            ("company_name", mapping.company_name.as_deref(), Some(self.source.name.as_str())),
            (
                "company_description",
                mapping.company_description.as_deref(),
                self.source.description.as_deref(),
            ),
        ] {
            self.field_origins.insert(field.into(), edited_text_origin(value, source));
        }
        self.mapping = mapping;
        Ok(())
    }

    /// 保存单项供给补值，接口采购报价须与已核验来源完全一致。
    /// # 参数
    /// `terms` 为完整供给草稿；调用方负责使该项及批次的旧预览失效。
    /// # 返回
    /// 保存供给补值及人工填写或明确清空标记。
    /// # 错误
    /// 已确认执行、来源报价缺失或试图修改接口采购报价时拒绝。
    pub fn edit_terms(&mut self, terms: SupplierOfferingTermsWrite) -> Result<()> {
        if !self.editable() {
            return Err(Error::ConflictError("该项已确认执行或结果待核实，不能调整供给".into()));
        }
        self.source.ensure_purchase_price(terms.dropship_supply_price_gross.trim().parse()?)?;
        for (field, value) in [("input_tax_rate", &terms.input_tax_rate), ("valid_from", &terms.valid_from)] {
            let origin = if value.trim().is_empty() { ValueOrigin::Cleared } else { ValueOrigin::Manual };
            self.field_origins.insert(field.into(), origin);
        }
        let origin = if terms.valid_to.as_deref().is_none_or(|value| value.trim().is_empty()) {
            ValueOrigin::Cleared
        } else {
            ValueOrigin::Manual
        };
        self.field_origins.insert("valid_to".into(), origin);
        self.terms = Some(terms);
        Ok(())
    }

    /// 依照填值策略应用资料默认值，保留人工值和明确清空。
    /// # 参数
    /// `defaults` 为本次公司分类和供给补值，`policy` 为明确覆盖范围。
    /// # 返回
    /// 将允许补值的字段保存为批量来源。
    /// # 错误
    /// 非可编辑行或映射结构不合法时拒绝。
    pub fn apply_defaults(&mut self, defaults: &BatchDefaults, policy: FillPolicy) -> Result<()> {
        if !self.editable() {
            return Err(Error::ConflictError("该项已确认执行，不能应用资料补值".into()));
        }
        defaults.validate()?;
        let keeps_product = self.resolution.as_ref().is_some_and(|value| value.kind != "CREATE_PRODUCT");
        if !keeps_product && let Some(category) = defaults.category_for(&self.source) {
            let origin = self.field_origin("category_id", !self.mapping.category_id.is_empty());
            if policy.allows(origin) {
                self.mapping.category_id = category.into();
                self.field_origins.insert("category_id".into(), ValueOrigin::Rule);
            }
        }
        self.apply_terms(defaults, policy);
        if let Some(template) = &defaults.delivery_template_id {
            let origin = self.field_origin("delivery_template_id", self.delivery_template_id.is_some());
            if policy.allows(origin) {
                self.delivery_template_id = Some(template.clone());
                self.field_origins.insert("delivery_template_id".into(), ValueOrigin::Rule);
            }
        }
        self.apply_clears(defaults, policy, keeps_product);
        Ok(())
    }

    fn apply_clears(&mut self, defaults: &BatchDefaults, policy: FillPolicy, keeps_product: bool) {
        for field in &defaults.clear_fields {
            let populated = match field.as_str() {
                "category_id" => !self.mapping.category_id.is_empty(),
                "delivery_template_id" => self.delivery_template_id.is_some(),
                "input_tax_rate" => self.terms.as_ref().is_some_and(|terms| !terms.input_tax_rate.is_empty()),
                "valid_from" => self.terms.as_ref().is_some_and(|terms| !terms.valid_from.is_empty()),
                "valid_to" => self.terms.as_ref().is_some_and(|terms| terms.valid_to.is_some()),
                _ => continue,
            };
            if (field == "category_id" && keeps_product)
                || !policy.allows(self.field_origin(field, populated))
            {
                continue;
            }
            match field.as_str() {
                "category_id" => self.mapping.category_id.clear(),
                "delivery_template_id" => self.delivery_template_id = None,
                _ => {
                    if let Some(terms) = &mut self.terms {
                        match field.as_str() {
                            "input_tax_rate" => terms.input_tax_rate.clear(),
                            "valid_from" => terms.valid_from.clear(),
                            "valid_to" => terms.valid_to = None,
                            _ => {},
                        }
                    }
                },
            }
            self.field_origins.insert(field.clone(), ValueOrigin::Cleared);
        }
    }

    fn field_origin(&self, field: &str, populated: bool) -> ValueOrigin {
        self.field_origins.get(field).copied().unwrap_or(if populated {
            ValueOrigin::Source
        } else {
            ValueOrigin::Unset
        })
    }

    fn apply_terms(&mut self, defaults: &BatchDefaults, policy: FillPolicy) {
        let terms = self.terms.get_or_insert_with(|| SupplierOfferingTermsWrite {
            dropship_supply_price_gross: self.source.purchase_price.clone().unwrap_or_default(),
            bulk_supply_price_gross: None,
            input_tax_rate: String::new(),
            bulk_minimum_order_quantity: None,
            product_capabilities: vec![],
            valid_from: String::new(),
            valid_to: None,
        });
        for (field, incoming, value) in [
            ("input_tax_rate", &defaults.input_tax_rate, &mut terms.input_tax_rate),
            ("valid_from", &defaults.valid_from, &mut terms.valid_from),
        ] {
            let origin = self.field_origins.get(field).copied().unwrap_or(if value.is_empty() {
                ValueOrigin::Unset
            } else {
                ValueOrigin::Source
            });
            if let Some(incoming) = incoming
                && policy.allows(origin)
            {
                *value = incoming.clone();
                self.field_origins.insert(field.into(), ValueOrigin::Rule);
            }
        }
        let origin = self.field_origins.get("valid_to").copied().unwrap_or(if terms.valid_to.is_some() {
            ValueOrigin::Source
        } else {
            ValueOrigin::Unset
        });
        if let Some(value) = &defaults.valid_to
            && policy.allows(origin)
        {
            terms.valid_to = Some(value.clone());
            self.field_origins.insert("valid_to".into(), ValueOrigin::Rule);
        }
    }

    /// 判断该项是否仍可修改；成功与待核实结果必须保留。
    /// # 参数
    /// 无。
    /// # 返回
    /// 待补、可提交、明确失败或已排除项返回真。
    /// # 错误
    /// 不返回错误。
    pub fn editable(&self) -> bool {
        matches!(
            self.status,
            BatchItemStatus::Draft
                | BatchItemStatus::Ready
                | BatchItemStatus::Failed
                | BatchItemStatus::Excluded
        ) && self.operation_id.is_none()
    }

    /// 单项修改取消旧匹配和提交资格，不改变已冻结的原操作。
    /// # 参数
    /// 无。
    /// # 返回
    /// 草稿恢复为待核对。
    /// # 错误
    /// 正在执行、结果待核实或已经完成时拒绝。
    pub fn invalidate_preview(&mut self) -> Result<()> {
        if !self.editable() {
            return Err(Error::ConflictError("该项已确认执行或结果待核实，不能修改原操作".into()));
        }
        self.status = BatchItemStatus::Draft;
        self.preview_version = None;
        self.frozen_row = None;
        self.warnings_confirmed = false;
        Ok(())
    }

    /// 为已核对项冻结原操作和真实载荷，进入后台队列。
    /// # 参数
    /// `operation_id` 为本次稳定操作号，`preview_version` 为已确认预览版本。
    /// # 返回
    /// 行保持可提交状态并保存原操作；后台通过 CAS 领取。
    /// # 错误
    /// 待补、未知、已完成、未确认价格提示或预览过期时拒绝。
    pub fn freeze(&mut self, operation_id: String, preview_version: u64) -> Result<()> {
        if self.status != BatchItemStatus::Ready
            || self.preview_version != Some(preview_version)
            || self.frozen_row.is_none()
            || (!self.warnings.is_empty() && !self.warnings_confirmed)
            || self.operation_id.is_some()
            || operation_id.trim().is_empty()
        {
            return Err(Error::ConflictError("该项尚未完成当前核对或已确认执行".into()));
        }
        self.operation_id = Some(operation_id);
        Ok(())
    }

    /// 领取已确认行；持久化 CAS 成功后才能执行实际业务写入。
    /// # 参数
    /// 无。
    /// # 返回
    /// 行变为处理中。
    /// # 错误
    /// 未确认、未知或已被其他执行器领取时拒绝。
    pub fn begin(&mut self) -> Result<()> {
        if self.status != BatchItemStatus::Ready || self.operation_id.is_none() || self.frozen_row.is_none() {
            return Err(Error::ConflictError("该项未确认或已被领取，不能重复执行".into()));
        }
        self.status = BatchItemStatus::Processing;
        self.processing_started_at = Some(Instant::now().unix_secs());
        Ok(())
    }

    /// 执行中断后保存待核实，保持原操作号及冻结载荷。
    /// # 参数
    /// `message` 为面向用户的查证原因。
    /// # 返回
    /// 行变为结果待核实。
    /// # 错误
    /// 不是正在执行的原操作时拒绝。
    pub fn mark_unknown(&mut self, message: String) -> Result<()> {
        if self.status != BatchItemStatus::Processing {
            return Err(Error::ConflictError("只有执行中的原操作可以登记为待核实".into()));
        }
        self.status = BatchItemStatus::Unknown;
        self.messages = vec![message];
        Ok(())
    }

    /// 明确失败后保留原因并解除本次执行冻结，必须重新预览再提交。
    /// # 参数
    /// `message` 为已证明未成功的错误原因。
    /// # 返回
    /// 行变为明确失败；原业务回执仍由永久回执集合保存。
    /// # 错误
    /// 结果待核实时拒绝清除原操作。
    pub fn mark_failed(&mut self, message: String) -> Result<()> {
        if self.status != BatchItemStatus::Processing {
            return Err(Error::ConflictError("当前结果不能作为明确失败重试".into()));
        }
        self.status = BatchItemStatus::Failed;
        self.messages = vec![message];
        self.operation_id = None;
        self.preview_version = None;
        Ok(())
    }

    /// 仅根据正式成功回执保存业务结果，支持原操作查证完成。
    /// # 参数
    /// `result` 为原子业务事务写入的永久回执结果。
    /// # 返回
    /// 行变为成功并保留原操作。
    /// # 错误
    /// 回执来源不符或当前行不是执行及查证状态时拒绝。
    pub fn complete(&mut self, result: AdoptionResult) -> Result<()> {
        if !matches!(self.status, BatchItemStatus::Processing | BatchItemStatus::Unknown)
            || result.item_id != self.source.base.id
        {
            return Err(Error::ConflictError("入库结果与原操作来源不一致".into()));
        }
        self.status = BatchItemStatus::Succeeded;
        self.messages.clear();
        self.result = Some(result);
        Ok(())
    }
}

fn edited_text_origin(value: Option<&str>, source: Option<&str>) -> ValueOrigin {
    match value {
        Some(value) if value.trim().is_empty() => ValueOrigin::Cleared,
        Some(_) => ValueOrigin::Manual,
        None if source.is_some_and(|value| !value.trim().is_empty()) => ValueOrigin::Source,
        None => ValueOrigin::Unset,
    }
}

/// 按互斥状态计数，每项只计入一个结果。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchCounts {
    pub total: usize,
    pub draft: usize,
    pub ready: usize,
    pub processing: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub unknown: usize,
    pub skipped: usize,
    pub excluded: usize,
}

impl BatchCounts {
    /// 汇总完整冻结成员集合的互斥状态。
    /// # 参数
    /// `items` 为同批次所有行，不得用当前页替代全部成员。
    /// # 返回
    /// 总量与各互斥状态数量。
    /// # 错误
    /// 不返回错误。
    pub fn from_items<'a>(items: impl IntoIterator<Item = &'a CatalogAdoptionBatchItem>) -> Self {
        let mut counts = Self::default();
        for item in items {
            counts.total += 1;
            match item.status {
                BatchItemStatus::Draft => counts.draft += 1,
                BatchItemStatus::Ready => counts.ready += 1,
                BatchItemStatus::Processing => counts.processing += 1,
                BatchItemStatus::Succeeded => counts.succeeded += 1,
                BatchItemStatus::Failed => counts.failed += 1,
                BatchItemStatus::Unknown => counts.unknown += 1,
                BatchItemStatus::Skipped => counts.skipped += 1,
                BatchItemStatus::Excluded => counts.excluded += 1,
            }
        }
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog_adoption::AdoptionTarget;
    use crate::ports::connector::common::{Snapshot, SourceRevision, SourceStamp, SupplierSku};
    use crate::ports::connector::offer::{Offer, OfferDetails};

    fn item() -> CatalogAdoptionBatchItem {
        let source = SupplierCatalogItem::observed(
            "connection",
            "supplier",
            "job",
            Snapshot {
                stamp: SourceStamp {
                    revision: SourceRevision::Unversioned,
                    changed_at: None,
                    observed_at: Instant::from_unix_secs(1),
                },
                value: Offer {
                    sku: SupplierSku { product_id: Some("product".into()), spec_id: "spec".into() },
                    name: "大米".into(),
                    specification: "5kg".into(),
                    unit: Some("袋".into()),
                    quote: None,
                    details: OfferDetails::default(),
                },
            },
        )
        .unwrap();
        CatalogAdoptionBatchItem {
            base: BaseModel::fake(),
            batch_id: "batch".into(),
            source,
            previous_source: None,
            initially_adopted: false,
            mapping: AdoptionMapping {
                category_id: String::new(),
                unit_id: String::new(),
                product_kind: "PHYSICAL".into(),
                company_name: None,
                company_description: None,
            },
            terms: None,
            sales_prices: BatchSalesValues::default(),
            delivery_template_id: None,
            field_origins: BTreeMap::new(),
            status: BatchItemStatus::Draft,
            messages: vec![],
            warnings: vec![],
            warnings_confirmed: false,
            resolution: None,
            frozen_row: None,
            result: None,
            operation_id: None,
            submitted_by: None,
            processing_started_at: None,
            preview_version: None,
        }
    }

    fn ready_item() -> CatalogAdoptionBatchItem {
        let mut item = item();
        item.apply_defaults(
            &BatchDefaults {
                input_tax_rate: Some("0.13".into()),
                valid_from: Some("2026-10-27".into()),
                ..Default::default()
            },
            FillPolicy::default(),
        )
        .unwrap();
        item.status = BatchItemStatus::Ready;
        item.preview_version = Some(4);
        item.frozen_row = Some(AdoptionRow {
            item_id: item.source.base.id.clone(),
            target: AdoptionTarget::Auto { mapping: item.mapping.clone(), expected_match: "match".into() },
            terms: item.terms.clone().unwrap(),
            sales_prices: None,
            delivery_template_id: None,
        });
        item
    }

    #[test]
    fn frozen_execution_is_claimed_once_and_unknown_never_replays() {
        let mut item = ready_item();
        assert!(item.freeze("operation".into(), 3).is_err());
        item.freeze("operation".into(), 4).unwrap();
        assert!(!item.editable());
        item.begin().unwrap();
        assert!(item.processing_started_at.is_some());
        assert!(item.begin().is_err());
        item.mark_unknown("请求结果待核实".into()).unwrap();
        assert_eq!(item.operation_id.as_deref(), Some("operation"));
        assert!(item.mark_failed("重新提交".into()).is_err());
        assert!(item.invalidate_preview().is_err());
        assert!(item.begin().is_err());
        let result = AdoptionResult {
            item_id: item.source.base.id.clone(),
            product_id: "p".into(),
            sku_id: "sku".into(),
            offering_id: "offer".into(),
        };
        item.complete(result).unwrap();
        assert_eq!(item.status, BatchItemStatus::Succeeded);
        assert!(!item.editable());
    }

    #[test]
    fn explicit_failure_requires_new_preview_and_warning_confirmation() {
        let mut item = ready_item();
        item.warnings.push("售价低于采购报价".into());
        assert!(item.freeze("operation".into(), 4).is_err());
        item.warnings_confirmed = true;
        item.freeze("operation".into(), 4).unwrap();
        item.begin().unwrap();
        item.mark_failed("分类已停用".into()).unwrap();
        assert!(item.editable());
        assert!(item.begin().is_err());
        assert!(item.freeze("new-operation".into(), 4).is_err());
        item.invalidate_preview().unwrap();
        assert_eq!(item.status, BatchItemStatus::Draft);
        assert!(item.frozen_row.is_none());
    }

    #[test]
    fn default_merge_preserves_manual_clear_and_current_product_category() {
        let mut item = item();
        item.mapping.category_id = "manual-category".into();
        item.field_origins.insert("category_id".into(), ValueOrigin::Manual);
        let defaults = BatchDefaults {
            category_id: Some("batch-category".into()),
            input_tax_rate: Some("0.13".into()),
            valid_from: Some("2026-10-27".into()),
            ..Default::default()
        };
        item.apply_defaults(&defaults, FillPolicy { overwrite: true, preserve_manual: true }).unwrap();
        assert_eq!(item.mapping.category_id, "manual-category");
        assert_eq!(item.terms.as_ref().unwrap().dropship_supply_price_gross, "");
        assert_eq!(item.terms.as_ref().unwrap().input_tax_rate, "0.13");
        let clear = BatchDefaults { clear_fields: vec!["input_tax_rate".into()], ..Default::default() };
        item.apply_defaults(&clear, FillPolicy { overwrite: true, preserve_manual: false }).unwrap();
        assert!(item.terms.as_ref().unwrap().input_tax_rate.is_empty());
        assert_eq!(item.field_origins["input_tax_rate"], ValueOrigin::Cleared);
        item.apply_defaults(&defaults, FillPolicy { overwrite: true, preserve_manual: true }).unwrap();
        assert!(item.terms.as_ref().unwrap().input_tax_rate.is_empty());
        item.resolution = Some(BatchResolution {
            kind: "CREATE_SKU".into(),
            expected_match: "m".into(),
            sku_no: None,
            product_id: Some("p".into()),
            reason: String::new(),
            group_anchor: None,
        });
        item.apply_defaults(&defaults, FillPolicy { overwrite: true, preserve_manual: false }).unwrap();
        assert_eq!(item.mapping.category_id, "manual-category");
    }

    #[test]
    fn category_mapping_and_mutually_exclusive_counts_remain_exact() {
        let mut first = item();
        first.source.category_id = Some("rice".into());
        let defaults = BatchDefaults {
            category_id: Some("fallback".into()),
            category_mappings: vec![BatchCategoryMapping {
                source_category_id: Some("rice".into()),
                category_id: "food".into(),
            }],
            ..Default::default()
        };
        first.apply_defaults(&defaults, FillPolicy::default()).unwrap();
        assert_eq!(first.mapping.category_id, "food");
        let mut items = vec![];
        for status in [
            BatchItemStatus::Draft,
            BatchItemStatus::Ready,
            BatchItemStatus::Processing,
            BatchItemStatus::Succeeded,
            BatchItemStatus::Failed,
            BatchItemStatus::Unknown,
            BatchItemStatus::Skipped,
            BatchItemStatus::Excluded,
        ] {
            let mut row = first.clone();
            row.status = status;
            items.push(row);
        }
        assert_eq!(
            BatchCounts::from_items(&items),
            BatchCounts {
                total: 8,
                draft: 1,
                ready: 1,
                processing: 1,
                succeeded: 1,
                failed: 1,
                unknown: 1,
                skipped: 1,
                excluded: 1
            }
        );
        let invalid = BatchDefaults {
            category_id: Some("c".into()),
            clear_fields: vec!["category_id".into()],
            ..Default::default()
        };
        assert!(invalid.validate().is_err());
        let invalid = BatchDefaults { clear_fields: vec!["unit_id".into()], ..Default::default() };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn invalidation_preserves_previous_match_for_existing_product_protection() {
        let mut item = ready_item();
        item.mapping.category_id = "current-category".into();
        item.resolution = Some(BatchResolution {
            kind: "EXISTING".into(),
            expected_match: "existing-match".into(),
            sku_no: Some("SKU-1".into()),
            product_id: Some("company-product".into()),
            reason: "关联已有 SKU".into(),
            group_anchor: None,
        });
        item.invalidate_preview().unwrap();
        assert!(item.preview_version.is_none());
        assert!(item.frozen_row.is_none());
        assert_eq!(item.resolution.as_ref().unwrap().expected_match, "existing-match");
        item.apply_defaults(
            &BatchDefaults { category_id: Some("wrong-category".into()), ..Default::default() },
            FillPolicy { overwrite: true, preserve_manual: false },
        )
        .unwrap();
        assert_eq!(item.mapping.category_id, "current-category");
    }

    #[test]
    fn uncategorized_mapping_does_not_become_other_categories_default() {
        let mut row = item();
        let mut defaults = BatchDefaults {
            category_mappings: vec![BatchCategoryMapping {
                source_category_id: None,
                category_id: "uncategorized-company".into(),
            }],
            ..Default::default()
        };
        assert!(defaults.validate().is_ok());
        assert_eq!(defaults.category_for(&row.source), Some("uncategorized-company"));
        row.source.category_id = Some("rice".into());
        assert_eq!(defaults.category_for(&row.source), None);
        defaults.category_id = Some("explicit-default".into());
        assert_eq!(defaults.category_for(&row.source), Some("explicit-default"));
        defaults
            .category_mappings
            .push(BatchCategoryMapping { source_category_id: None, category_id: "duplicate".into() });
        assert!(defaults.validate().is_err());
        defaults.category_mappings.pop();
        defaults.category_mappings.push(BatchCategoryMapping {
            source_category_id: Some(" ".into()),
            category_id: "company".into(),
        });
        assert!(defaults.validate().is_err());
    }

    fn batch() -> CatalogAdoptionBatch {
        CatalogAdoptionBatch {
            base: BaseModel::fake(),
            batch_no: "SA202610270001".into(),
            connection_id: "connection".into(),
            supplier_id: "supplier".into(),
            actor_id: "actor".into(),
            selection_filter: None,
            selected_count: 1,
            excluded_ids: vec![],
            rules: BatchPricing::default(),
            fill_policy: FillPolicy::default(),
            defaults: BatchDefaults::default(),
            step: "REVIEW".into(),
            preview_version: Some(4),
            submitted: false,
            selection_complete: false,
            execution_pending: false,
            confirmations: vec![],
            draft_operation: None,
            draft_started_at: None,
            inactive_pricing: None,
            published_operations: vec![],
        }
    }

    #[test]
    fn unfinished_selection_cannot_submit_and_old_optional_fields_fail_closed() {
        let mut batch = batch();
        let ids = vec!["source-item".into()];
        assert!(batch.ensure_submission(4, &ids).is_err());
        batch.selection_complete = true;
        assert!(batch.ensure_submission(4, &ids).is_ok());
        batch.draft_operation = Some("saving".into());
        assert!(batch.ensure_submission(4, &ids).is_err());
        batch.draft_operation = None;
        let mut encoded = serde_json::to_value(&batch).unwrap();
        encoded.as_object_mut().unwrap().remove("selection_complete");
        encoded.as_object_mut().unwrap().remove("execution_pending");
        encoded.as_object_mut().unwrap().remove("confirmations");
        encoded.as_object_mut().unwrap().remove("draft_operation");
        encoded.as_object_mut().unwrap().remove("draft_started_at");
        encoded.as_object_mut().unwrap().remove("inactive_pricing");
        encoded.as_object_mut().unwrap().remove("published_operations");
        let old: CatalogAdoptionBatch = serde_json::from_value(encoded).unwrap();
        assert!(!old.selection_complete);
        assert!(!old.execution_pending);
        assert!(old.confirmations.is_empty());
        assert!(old.draft_operation.is_none());
        assert!(old.draft_started_at.is_none());
        assert!(old.inactive_pricing.is_none());
        assert!(old.published_operations.is_empty());
        let mut encoded = serde_json::to_value(item()).unwrap();
        encoded.as_object_mut().unwrap().remove("previous_source");
        encoded.as_object_mut().unwrap().remove("submitted_by");
        let old: CatalogAdoptionBatchItem = serde_json::from_value(encoded).unwrap();
        assert!(old.previous_source.is_none());
        assert!(old.submitted_by.is_none());
    }

    #[test]
    fn draft_takeover_respects_expiry_future_time_and_saturating_boundaries() {
        let mut batch = batch();
        assert!(batch.ensure_draft_available(100).is_ok());
        batch.draft_operation = Some("original".into());
        assert!(batch.ensure_draft_available(100).is_ok());
        batch.draft_started_at = Some(100);
        for now in [i64::MIN, 99, 100, 399] {
            assert!(batch.ensure_draft_available(now).is_err());
        }
        for now in [400, 401, i64::MAX] {
            assert!(batch.ensure_draft_available(now).is_ok());
        }
        batch.draft_started_at = Some(i64::MAX);
        assert!(batch.ensure_draft_available(i64::MIN).is_err());
        batch.draft_started_at = Some(i64::MIN);
        assert!(batch.ensure_draft_available(i64::MAX).is_ok());
        batch.draft_operation = None;
        assert!(batch.ensure_draft_available(i64::MIN).is_ok());
    }

    #[test]
    fn draft_operation_requires_exact_original_token() {
        let mut batch = batch();
        assert!(batch.ensure_draft_operation("original").is_err());
        batch.draft_operation = Some("original".into());
        assert!(batch.ensure_draft_operation("original").is_ok());
        for token in ["", "ORIGINAL", "original ", "different"] {
            assert!(batch.ensure_draft_operation(token).is_err());
        }
        batch.draft_operation = Some("replacement".into());
        assert!(batch.ensure_draft_operation("original").is_err());
    }

    #[test]
    fn single_mapping_edit_records_manual_clear_and_source_origins() {
        let mut row = ready_item();
        let mut mapping = row.mapping.clone();
        mapping.category_id = " ".into();
        mapping.unit_id = "unit".into();
        mapping.company_name = None;
        mapping.company_description = None;
        row.edit_mapping(mapping.clone()).unwrap();
        assert_eq!(row.field_origins["category_id"], ValueOrigin::Cleared);
        assert_eq!(row.field_origins["unit_id"], ValueOrigin::Manual);
        assert_eq!(row.field_origins["company_name"], ValueOrigin::Source);
        assert_eq!(row.field_origins["company_description"], ValueOrigin::Unset);
        assert_eq!(row.preview_version, Some(4));
        mapping.category_id = "company-category".into();
        mapping.unit_id.clear();
        mapping.company_name = Some(String::new());
        mapping.company_description = Some("公司介绍".into());
        row.edit_mapping(mapping.clone()).unwrap();
        assert_eq!(row.field_origins["category_id"], ValueOrigin::Manual);
        assert_eq!(row.field_origins["unit_id"], ValueOrigin::Cleared);
        assert_eq!(row.field_origins["company_name"], ValueOrigin::Cleared);
        assert_eq!(row.field_origins["company_description"], ValueOrigin::Manual);
        assert_eq!(row.mapping.company_description.as_deref(), Some("公司介绍"));
        row.source.description = Some("来源介绍".into());
        mapping.company_description = None;
        row.edit_mapping(mapping.clone()).unwrap();
        assert_eq!(row.field_origins["company_description"], ValueOrigin::Source);
        row.freeze("operation".into(), 4).unwrap();
        assert!(row.edit_mapping(mapping).is_err());
    }

    #[test]
    fn single_terms_edit_preserves_purchase_quote_and_records_explicit_clears() {
        let mut row = ready_item();
        let mut terms = row.terms.clone().unwrap();
        terms.dropship_supply_price_gross = "0".into();
        assert!(row.edit_terms(terms.clone()).is_err());
        row.source.purchase_price = Some("0".into());
        terms.input_tax_rate.clear();
        terms.valid_from.clear();
        terms.valid_to = None;
        row.edit_terms(terms.clone()).unwrap();
        for field in ["input_tax_rate", "valid_from", "valid_to"] {
            assert_eq!(row.field_origins[field], ValueOrigin::Cleared);
        }
        terms.input_tax_rate = "0.13".into();
        terms.valid_from = "2026-10-27".into();
        terms.valid_to = Some("2027-10-27".into());
        row.edit_terms(terms.clone()).unwrap();
        for field in ["input_tax_rate", "valid_from", "valid_to"] {
            assert_eq!(row.field_origins[field], ValueOrigin::Manual);
        }
        terms.dropship_supply_price_gross = "1".into();
        assert!(row.edit_terms(terms.clone()).is_err());
        assert_eq!(row.terms.as_ref().unwrap().dropship_supply_price_gross, "0");
        row.status = BatchItemStatus::Unknown;
        terms.dropship_supply_price_gross = "0".into();
        assert!(row.edit_terms(terms).is_err());
    }
}
