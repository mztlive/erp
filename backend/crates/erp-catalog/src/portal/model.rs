//! 供应商新品提报的原始快照、审核映射与状态规则。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::money::Quantity;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::CategoryHierarchyNode;
use crate::entity::catalog::{ListingStatus, ProductKind};
use crate::{Error, Result, SpecEntryInput};

#[path = "packaging.rs"]
mod packaging;
#[path = "validation.rs"]
mod validation;

pub use packaging::PackagingInput;
use validation::{decision, ensure_result, required, validate_id, validation};

/// 当前内部审核人的完整生效命令，匹配与建档理由不进入供应商原稿。
#[derive(Debug, Clone)]
pub struct CatalogDraftEffectiveCommand {
    pub draft_id: String,
    pub expected_version: u64,
    pub normalized: NormalizedProduct,
    pub result: CatalogDraftResult,
    pub actor_id: String,
    pub reason: String,
}

/// 供应商实际填写的字典值；未匹配时仍保留原始名称或路径。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DictionaryInput {
    /// 原始文本或提交时展示的名称；分类可记录完整原始路径。
    pub raw_name: String,
    /// 明确选择的字典标识；未匹配不得伪造标识。
    pub selected_id: Option<String>,
    /// 所选项的提交时版本，与 selected_id 成对。
    pub expected_version: Option<u64>,
}

/// 新品中一条 SKU 的供应商原始资料。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftSku {
    /// 申请内稳定行标识，不使用公司 SKU 编号。
    pub row_id: String,
    /// 供应商原始 SKU 名称。
    pub name: String,
    /// 商品内规格属性和值。
    #[serde(default)]
    pub spec_entries: Vec<SpecEntryInput>,
    /// 原始计量单位；包含必要单位含义及包装说明。
    pub unit: DictionaryInput,
    /// 供应商明确填写并确认的原始包装关系；后台不自动换算。
    #[serde(default)]
    pub packaging: Option<PackagingInput>,
    /// 原始报价单位、包装及计价依据的补充说明。
    #[serde(default)]
    pub quote_basis: Option<String>,
    pub barcode: Option<String>,
    pub image_asset_id: Option<String>,
    /// 当前供应商的订货编码。
    pub ordering_code: String,
    /// 首次供给条款；由供给领域 Port 校验其内部结构与商务规则。
    pub supply_terms: Value,
    /// 空值明确表示供应商未提供可供数量。
    pub available_quantity: Option<Quantity>,
    /// 供应商实际填报时间，不得以审核时间替代。
    pub reported_at: Instant,
}

/// 一个商品及其全部 SKU 的供应商提报原稿。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewProductInput {
    pub name: String,
    pub product_kind: ProductKind,
    pub brand: DictionaryInput,
    pub category: DictionaryInput,
    pub model: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub image_asset_ids: Vec<String>,
    #[serde(default)]
    pub file_asset_ids: Vec<String>,
    #[serde(default)]
    pub skus: Vec<DraftSku>,
}

/// 明确匹配的已有 SKU 身份及审核依据。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExistingSkuRef {
    pub sku_id: String,
    pub version: u64,
    pub revision_id: String,
}

/// 内部确认的一条 SKU 映射，不提供规格、条款或包装改写入口。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NormalizedSku {
    pub row_id: String,
    /// 仅允许对供应商名称去首尾空白。
    pub name: String,
    pub unit_id: String,
    pub unit_version: u64,
    /// 单位原文与正式字典不同的显式同义核对，不进行数量或报价换算。
    #[serde(default)]
    pub unit_synonym_confirmation: Option<UnitSynonymConfirmation>,
    pub target_sku: Option<ExistingSkuRef>,
}

/// 内部确认两种单位文本表示同一基础单位，必须保留原文和核对依据。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitSynonymConfirmation {
    pub original_unit: String,
    pub same_unit_meaning_confirmed: bool,
    pub reason: String,
}

/// 单独保存的内部建档映射；供应商原稿保持完整。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NormalizedProduct {
    pub name: String,
    pub brand_id: String,
    pub brand_version: u64,
    pub category_id: String,
    pub category_version: u64,
    /// 人工核对时从根到叶的完整分类事实，生效时逐级重验。
    pub category_hierarchy: Vec<CategoryHierarchyNode>,
    pub sku_mappings: Vec<NormalizedSku>,
}

/// 一条实际建档结果；供给编号在跨域编排完成时填入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogDraftSkuResult {
    pub row_id: String,
    pub sku_id: String,
    pub revision_id: String,
    pub sku_created: bool,
    pub listing_status: ListingStatus,
    pub offering_id: Option<String>,
}

/// 本次商品、全部 SKU 及供给的实际结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogDraftResult {
    pub product_id: String,
    pub product_created: bool,
    pub skus: Vec<CatalogDraftSkuResult>,
}

/// 提报单的主状态；与供给、可供和上架状态分别保存。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DraftStatus {
    Draft,
    Pending,
    Returned,
    Withdrawn,
    Effective,
}

impl DraftStatus {
    /// 返回提报状态的稳定代码。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回持久化和界面传输使用的 snake_case 状态。
    /// # 错误
    /// 无。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Pending => "pending",
            Self::Returned => "returned",
            Self::Withdrawn => "withdrawn",
            Self::Effective => "effective",
        }
    }
}

/// 一次提交的最终决定及完整关联。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftDecision {
    pub status: DraftStatus,
    pub reason: Option<String>,
    pub decided_by: String,
    pub decided_at: Instant,
    pub normalized_product: Option<NormalizedProduct>,
    pub result: Option<CatalogDraftResult>,
}

/// 不可原地修改的历次提交及决定。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftSubmission {
    pub id: String,
    pub input: NewProductInput,
    pub submitted_by: String,
    pub submitted_at: Instant,
    /// 历史任务关联，在重新提交后仍保留。
    pub task_id: String,
    pub decision: Option<DraftDecision>,
}

/// 独立于正式商品和供给的新品暂存单。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct NewProductDraft {
    #[serde(flatten)]
    pub base: BaseModel,
    pub supplier_id: String,
    pub created_by: String,
    pub draft: NewProductInput,
    pub status: DraftStatus,
    pub frozen_input: Option<NewProductInput>,
    pub submissions: Vec<DraftSubmission>,
    pub current_submission_id: Option<String>,
    pub task_id: Option<String>,
    /// 当前提交的内部映射，与供应商冻结原稿分别保存。
    #[serde(default)]
    pub normalized_product: Option<NormalizedProduct>,
    pub result: Option<CatalogDraftResult>,
}

impl NewProductDraft {
    /// 创建归属固定的新品草稿，不建立正式商品或供给。
    ///
    /// # 参数
    /// * `id` - 提报单稳定标识。
    /// * `supplier_id` - 服务端确认的供应商归属。
    /// * `created_by` - 创建人的外部账号标识。
    /// * `draft` - 可尚未填写完整的原始输入。
    /// # 返回
    /// 返回草稿状态的新品提报单。
    /// # 错误
    /// 归属、创建人或存储输入非法时返回验证错误。
    pub fn new(id: String, supplier_id: String, created_by: String, draft: NewProductInput) -> Result<Self> {
        validate_id(&id, "提报单")?;
        validate_id(&supplier_id, "供应商")?;
        validate_id(&created_by, "创建人")?;
        draft.validate_storage()?;
        Ok(Self {
            base: BaseModel::new(id),
            supplier_id,
            created_by,
            draft,
            status: DraftStatus::Draft,
            frozen_input: None,
            submissions: Vec::new(),
            current_submission_id: None,
            task_id: None,
            normalized_product: None,
            result: None,
        })
    }

    /// 更新尚未生效且未送审的原稿，保留历次提交。
    ///
    /// # 参数
    /// * `expected_version` - 读取草稿时的乐观锁版本。
    /// * `draft` - 本次完整替换的可编辑原稿。
    /// # 返回
    /// 更新成功返回空结果；仓储负责递增持久化版本。
    /// # 错误
    /// 版本冲突、待确认、生效、软删除或输入非法时拒绝更新。
    pub fn update(&mut self, expected_version: u64, draft: NewProductInput) -> Result<()> {
        self.ensure_editable(expected_version)?;
        draft.validate_storage()?;
        self.draft = draft;
        self.normalized_product = None;
        Ok(())
    }

    /// 冻结一份完整提交并关联本次单人确认任务。
    ///
    /// # 参数
    /// * `expected_version` - 当前提报版本。
    /// * `submission_id` - 本次提交稳定标识，不能复用历史标识。
    /// * `task_id` - 已登记的本次确认任务标识。
    /// * `submitted_by` - 实际供应商提交人。
    /// * `submitted_at` - 实际提交时间。
    /// # 返回
    /// 状态变为待确认，原稿及提交历史分别保存。
    /// # 错误
    /// 非可编辑状态、版本过期、输入不完整或关联标识重复时拒绝提交。
    pub fn submit(
        &mut self,
        expected_version: u64,
        submission_id: String,
        task_id: String,
        submitted_by: String,
        submitted_at: Instant,
    ) -> Result<()> {
        self.ensure_editable(expected_version)?;
        self.draft.validate_submission()?;
        self.draft.ensure_submission_images()?;
        validate_id(&submission_id, "提交")?;
        validate_id(&task_id, "确认任务")?;
        validate_id(&submitted_by, "提交人")?;
        if self
            .submissions
            .iter()
            .any(|submission| submission.id == submission_id || submission.task_id == task_id)
        {
            return Err(Error::ConflictError("提交标识或确认任务已被使用".into()));
        }
        self.submissions.push(DraftSubmission {
            id: submission_id.clone(),
            input: self.draft.clone(),
            submitted_by,
            submitted_at,
            task_id: task_id.clone(),
            decision: None,
        });
        self.frozen_input = Some(self.draft.clone());
        self.current_submission_id = Some(submission_id);
        self.task_id = Some(task_id);
        self.normalized_product = None;
        self.status = DraftStatus::Pending;
        Ok(())
    }

    /// 读取当前待确认的供应商冻结输入。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回本次冻结输入的借用，调用方不得原地修改。
    /// # 错误
    /// 非待确认状态或提交关联损坏时返回错误。
    pub fn submitted(&self) -> Result<&NewProductInput> {
        let id = self.current_submission_id.as_deref().ok_or_else(|| validation("新品尚未提交"))?;
        self.pending_index(id)?;
        self.frozen_input.as_ref().ok_or_else(|| Error::Internal("新品冻结输入缺失".into()))
    }

    /// 保存当前待确认提交的独立内部映射，保留供应商冻结原稿。
    ///
    /// # 参数
    /// * `expected_version` - 当前提报版本。
    /// * `normalized` - 本次内部明确确认的字典和目标 SKU 映射。
    /// # 返回
    /// 映射与原稿完全对应时保存并返回空结果。
    /// # 错误
    /// 版本过期、非待确认或映射不符合原稿规则时拒绝保存。
    pub fn map(&mut self, expected_version: u64, normalized: NormalizedProduct) -> Result<()> {
        self.ensure_version(expected_version)?;
        self.submitted()?.ensure_normalized(&normalized)?;
        self.normalized_product = Some(normalized);
        Ok(())
    }

    /// 撤回当前尚未完成的提交，保留原始输入及撤回记录。
    ///
    /// # 参数
    /// * `expected_version` - 当前提报版本。
    /// * `submission_id` - 当前待确认提交标识。
    /// * `actor_id` - 实际供应商操作人。
    /// * `at` - 撤回发生时间。
    /// # 返回
    /// 状态变为已撤回，当前任务解除关联。
    /// # 错误
    /// 版本或提交过期、已完成、非待确认或操作人无效时拒绝撤回。
    pub fn withdraw(
        &mut self,
        expected_version: u64,
        submission_id: &str,
        actor_id: &str,
        at: Instant,
    ) -> Result<()> {
        self.ensure_version(expected_version)?;
        let index = self.pending_index(submission_id)?;
        let decision = decision(DraftStatus::Withdrawn, None, actor_id, at)?;
        self.finish(index, decision);
        Ok(())
    }

    /// 将当前提交退回供应商，必须记录供应商可见原因。
    ///
    /// # 参数
    /// * `expected_version` - 当前提报版本。
    /// * `submission_id` - 本次待确认提交标识。
    /// * `reason` - 供应商可见的明确退回原因。
    /// * `actor_id` - 实际内部确认人。
    /// * `at` - 决定时间。
    /// # 返回
    /// 状态变为已退回并保存本次决定。
    /// # 错误
    /// 原因空白、版本冲突或提交不再待确认时拒绝退回。
    pub fn return_to_supplier(
        &mut self,
        expected_version: u64,
        submission_id: &str,
        reason: String,
        actor_id: &str,
        at: Instant,
    ) -> Result<()> {
        self.ensure_version(expected_version)?;
        let index = self.pending_index(submission_id)?;
        let reason = required(&reason, "退回原因", 2_000)?;
        let decision = decision(DraftStatus::Returned, Some(reason), actor_id, at)?;
        self.finish(index, decision);
        Ok(())
    }

    /// 保存原子建档编排已完成的全部结果和内部映射。
    ///
    /// # 参数
    /// * `submission_id` - 本次待确认提交标识。
    /// * `command` - 精确提报、版本、内部映射、全部正式结果、真实确认人和核对理由。
    /// * `at` - 审核决定时间，不替代供应商填报时间。
    /// # 返回
    /// 状态变为已生效并独立保留原稿、映射和最终结果。
    /// # 错误
    /// 缺真实核对理由、版本或状态冲突、映射改变含义、结果不完整或新 SKU 上架时拒绝。
    pub fn mark_effective(
        &mut self,
        submission_id: &str,
        command: CatalogDraftEffectiveCommand,
        at: Instant,
    ) -> Result<()> {
        if command.draft_id != self.base.id {
            return Err(validation("建档决定与新品提报身份不一致"));
        }
        self.ensure_version(command.expected_version)?;
        required(&command.reason, "匹配与建档核对说明", 500)?;
        let index = self.pending_index(submission_id)?;
        self.submitted()?.ensure_normalized(&command.normalized)?;
        ensure_result(&command.normalized, &command.result)?;
        let mut decision =
            decision(DraftStatus::Effective, Some(command.reason.trim().into()), &command.actor_id, at)?;
        decision.normalized_product = Some(command.normalized.clone());
        decision.result = Some(command.result.clone());
        self.normalized_product = Some(command.normalized);
        self.finish(index, decision);
        self.result = Some(command.result);
        Ok(())
    }

    /// 校验实体版本及活动状态，持久化更新另受仓储乐观锁保护。
    ///
    /// # 参数
    /// * `expected` - 调用方读取提报单时的版本。
    /// # 返回
    /// 当前活动提报版本一致时返回空结果。
    /// # 错误
    /// 软删除时返回不存在，版本缺失或不一致时返回冲突。
    pub fn ensure_version(&self, expected: u64) -> Result<()> {
        if self.base.is_deleted() {
            return Err(Error::NotFound("新品提报单".into()));
        }
        if expected == 0 || self.base.version != expected {
            return Err(Error::ConflictError("新品提报已变化，请刷新后重新核对".into()));
        }
        Ok(())
    }

    /// 仅草稿、退回和撤回可更新或重新提交。
    fn ensure_editable(&self, expected: u64) -> Result<()> {
        self.ensure_version(expected)?;
        if matches!(self.status, DraftStatus::Pending | DraftStatus::Effective) {
            return Err(validation("待确认或已生效新品不能修改或重新提交"));
        }
        Ok(())
    }

    /// 同时核对主状态、当前提交指针和历史提交决定。
    fn pending_index(&self, submission_id: &str) -> Result<usize> {
        if self.base.is_deleted() {
            return Err(Error::NotFound("新品提报单".into()));
        }
        if self.status != DraftStatus::Pending || self.current_submission_id.as_deref() != Some(submission_id)
        {
            return Err(Error::ConflictError("本次新品提交不再待确认".into()));
        }
        let index = self
            .submissions
            .iter()
            .position(|submission| submission.id == submission_id)
            .ok_or_else(|| Error::Internal("新品当前提交关联缺失".into()))?;
        if self.submissions[index].decision.is_some()
            || self.task_id.as_deref() != Some(self.submissions[index].task_id.as_str())
        {
            return Err(Error::Internal("新品当前提交决定或任务关联无效".into()));
        }
        Ok(index)
    }

    /// 调用方完成所有校验后一次性写入决定，避免失败产生部分状态。
    fn finish(&mut self, index: usize, mut decision: DraftDecision) {
        if decision.normalized_product.is_none() {
            decision.normalized_product = self.normalized_product.clone();
        }
        self.status = decision.status;
        self.submissions[index].decision = Some(decision);
        self.task_id = None;
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
