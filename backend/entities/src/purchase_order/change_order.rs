//! `purchase_change_order` / `purchase_change_submission`(+line)（数据模型 §6.6）。
//!
//! 采购变更单只适用于实物与服务销售单（phase-1 §6.3）；已入库、已付款和已形成
//! 发票的事实不回退，生效事务把已通过复核的目标提交原样复制为新采购版本、版本行
//! 和销售分配（§6.6 必需约束，P3 编排）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::stable::StableBase;
use crate::common::time::{BusinessDate, Instant};
use crate::errors::{Error, Result};
use crate::ids::{
    ProcurementConfirmationLineId, PurchaseChangeOrderId, PurchaseChangeSubmissionId,
    PurchaseChangeSubmissionLineId, PurchaseOrderId, PurchaseOrderRevisionId, SalesOrderLineId,
    SalesOrderRevisionLineId, SalesOrderSubmissionLineId, SkuId, SkuRevisionId, SupplierAccountId,
    SupplierCommercialProfileRevisionId,
};
use crate::money::{Amount, Quantity, Rate, UnitPrice};
use crate::purchase_order::line_common::{normalize_and_validate_line, PurchaseLineDataRef};
use crate::purchase_order::purchase_submission::SubmissionStatus;
use crate::purchase_order::snapshot::{PaymentTermSnapshot, SupplierSnapshot};
use crate::purchase_order::types::{FulfillmentResponsibility, PurchaseLineType, PurchaseType};
use crate::validation::normalize_required_text;

/// 变更原因最大长度。
const REASON_MAX_LEN: usize = 500;
/// 提交序号最大长度。
const SUBMISSION_NO_MAX_LEN: usize = 64;
/// 目标内容指纹最大长度。
const CONTENT_HASH_MAX_LEN: usize = 128;
/// 操作人标识最大长度。
const ACTOR_MAX_LEN: usize = 128;

/// 采购变更单状态（合同 §4.4.1 / §4.4.2：草稿、审批中、已生效、作废）。
///
/// 创建与提交均为 `Draft`，启动后 `InApproval`，最终通过 `Effective`。
/// `PENDING_WAREHOUSE_IMPACT`、`PENDING_FINANCE_REVIEW` 与审批导致的 `Rejected`
/// 已删除，节点事实只存在于审批实例。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PurchaseChangeOrderStatus {
    /// 草稿。
    Draft,
    /// 审批中。
    #[serde(rename = "IN_APPROVAL")]
    InApproval,
    /// 已生效。
    Effective,
    /// 作废。
    Voided,
}

impl PurchaseChangeOrderStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::InApproval => "审批中",
            Self::Effective => "已生效",
            Self::Voided => "作废",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::InApproval => "IN_APPROVAL",
            Self::Effective => "EFFECTIVE",
            Self::Voided => "VOIDED",
        }
    }

    /// 判断状态是否代表尚未结束的采购变更。
    ///
    /// # 返回
    /// 草稿或审批中返回 `true`，已生效或作废返回 `false`。
    pub fn is_in_progress(self) -> bool {
        matches!(self, Self::Draft | Self::InApproval)
    }
}

/// 采购变更单创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseChangeOrderData {
    /// 原采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 基准版本。
    pub base_revision_id: PurchaseOrderRevisionId,
    /// 采购变化原因。
    pub reason: String,
}

/// 采购变更单更新数据。
///
/// 内容编辑只允许在草稿状态（§7.4：生效后变化走变更单，变更单自身草稿可编辑）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PurchaseChangeOrderUpdate {
    /// 采购变化原因；`None` 表示不修改。
    pub reason: Option<String>,
    /// 当前不可变目标提交；`None` 表示不修改。
    pub current_submission_id: Option<PurchaseChangeSubmissionId>,
    /// 目标提交内容指纹；`None` 表示不修改。
    pub target_content_hash: Option<String>,
    /// 生效后形成的新采购版本；`None` 表示不修改。
    pub effective_revision_id: Option<PurchaseOrderRevisionId>,
}

/// 采购变更单实体（可编辑单据草稿，数据模型 §6.6）。
///
/// `StableBase` 未派生 `PartialEq`，因此本实体手工实现全字段语义相等。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct PurchaseChangeOrder {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<PurchaseChangeOrderStatus>,
    /// 原采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 基准版本。
    pub base_revision_id: PurchaseOrderRevisionId,
    /// 采购变化原因。
    pub reason: String,
    /// 当前不可变目标提交。
    pub current_submission_id: Option<PurchaseChangeSubmissionId>,
    /// 目标提交内容指纹。
    pub target_content_hash: Option<String>,
    /// 生效后形成的新采购版本。
    pub effective_revision_id: Option<PurchaseOrderRevisionId>,
    /// 审批提交版本，初值 0。
    #[serde(default)]
    pub approval_subject_version: u32,
}

impl PartialEq for PurchaseChangeOrder {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.purchase_order_id == other.purchase_order_id
            && self.base_revision_id == other.base_revision_id
            && self.reason == other.reason
            && self.current_submission_id == other.current_submission_id
            && self.target_content_hash == other.target_content_hash
            && self.effective_revision_id == other.effective_revision_id
            && self.approval_subject_version == other.approval_subject_version
    }
}

impl Eq for PurchaseChangeOrder {}

impl PurchaseChangeOrder {
    /// 创建采购变更单。
    ///
    /// 完成变更原因校验与规范化；初始状态为 `Draft`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::PurchaseChangeOrderId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的变更单实体。
    ///
    /// # 错误
    /// 变更原因为空或超长时返回错误。
    pub fn new(
        id: PurchaseChangeOrderId,
        data: PurchaseChangeOrderData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let reason = normalize_required_text(
            data.reason,
            "采购变化原因不能为空",
            REASON_MAX_LEN,
            "采购变化原因过长",
        )?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(PurchaseChangeOrderStatus::Draft, created_by),
            purchase_order_id: data.purchase_order_id,
            base_revision_id: data.base_revision_id,
            reason,
            current_submission_id: None,
            target_content_hash: None,
            effective_revision_id: None,
            approval_subject_version: 0,
        })
    }

    /// 校验调用方持有的乐观锁版本。
    ///
    /// # 参数
    /// * `expected` - 调用方读取到的期望版本
    ///
    /// # 返回
    /// 版本一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 期望版本与实体当前版本不一致时返回领域错误。
    pub fn ensure_expected_version(&self, expected: u64) -> Result<()> {
        if self.base.version != expected {
            return Err(Error::from("采购变更单版本已变化"));
        }
        Ok(())
    }

    /// 校验变更单仍可冻结新的审批提交。
    ///
    /// # 返回
    /// 草稿状态返回 `Ok(())`。
    ///
    /// # 错误
    /// 非草稿状态返回领域错误。
    pub fn ensure_draft_for_submission(&self) -> Result<()> {
        if self.stable.status != PurchaseChangeOrderStatus::Draft {
            return Err(Error::from("变更单已提交，请勿重复提交"));
        }
        Ok(())
    }

    /// 解析最终生效动作必须使用的当前冻结提交。
    ///
    /// # 参数
    /// * `requested` - 可选的调用方提交 ID；空值表示直接采用当前冻结提交
    ///
    /// # 返回
    /// 返回当前冻结提交的类型化稳定身份。
    ///
    /// # 错误
    /// 变更单尚未提交，或请求提交与当前冻结提交不一致时返回领域错误。
    pub fn submission_id_for_effect(&self, requested: Option<&str>) -> Result<PurchaseChangeSubmissionId> {
        let current = self
            .current_submission_id
            .clone()
            .ok_or_else(|| Error::from("变更单尚未提交审批"))?;
        if let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) {
            if requested != current.as_ref() {
                return Err(Error::from("生效提交必须是当前冻结提交，不得使用历史提交"));
            }
        }
        Ok(current)
    }

    /// 校验变更基准版本仍是采购单当前生效版本。
    ///
    /// # 参数
    /// * `current_revision_id` - 原采购单当前生效版本
    ///
    /// # 返回
    /// 基准版本一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 原采购单缺少当前版本，或当前版本已偏离变更基准时返回领域错误。
    pub fn ensure_base_revision_current(&self, current_revision_id: Option<&str>) -> Result<()> {
        if current_revision_id != Some(self.base_revision_id.as_ref()) {
            return Err(Error::from("基准版本已不是当前版本，变更不能生效"));
        }
        Ok(())
    }

    /// 更新采购变更单。
    ///
    /// 原因/目标提交等内容只允许在草稿状态编辑；状态与生效版本由
    /// P3 按 §6.6/§8.1 第 3 条编排（`purchase_order_id`、`base_revision_id` 不可修改）。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态不是草稿，或更新字段校验失败时返回错误。
    pub fn update(&mut self, update: PurchaseChangeOrderUpdate, updated_by: impl Into<String>) -> Result<()> {
        self.apply_content(&update, updated_by)
    }

    /// 应用内容更新（草稿门禁）。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 错误
    /// 状态不是草稿，或原因/内容指纹校验失败时返回错误。
    fn apply_content(
        &mut self,
        update: &PurchaseChangeOrderUpdate,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        if update_has_content(update) && self.stable.status != PurchaseChangeOrderStatus::Draft {
            return Err(Error::from("只有草稿状态的采购变更单可以编辑内容"));
        }
        if let Some(reason) = update.reason.clone() {
            self.reason =
                normalize_required_text(reason, "采购变化原因不能为空", REASON_MAX_LEN, "采购变化原因过长")?;
        }
        if let Some(submission_id) = update.current_submission_id.clone() {
            self.current_submission_id = Some(submission_id);
        }
        if let Some(hash) = update.target_content_hash.clone() {
            self.target_content_hash = Some(normalize_required_text(
                hash,
                "目标内容指纹不能为空",
                CONTENT_HASH_MAX_LEN,
                "目标内容指纹过长",
            )?);
        }
        if let Some(revision_id) = update.effective_revision_id.clone() {
            self.effective_revision_id = Some(revision_id);
        }
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 提交并启动审批：递增 `approval_subject_version` 并进入 `IN_APPROVAL`。
    ///
    /// 版本使用 checked add，成功后不回退。不得改写 `BaseModel.version`。
    ///
    /// # 参数
    /// * `submission_id` - 本次冻结的不可变目标提交
    /// * `target_content_hash` - 目标内容指纹
    /// * `updated_by` - 提交人
    ///
    /// # 返回
    /// 返回冻结后的提交版本。
    ///
    /// # 错误
    /// 非草稿、指纹非法或版本溢出时返回冲突。
    pub fn start_approval(
        &mut self,
        submission_id: PurchaseChangeSubmissionId,
        target_content_hash: impl Into<String>,
        updated_by: impl Into<String>,
    ) -> Result<u32> {
        if self.stable.status != PurchaseChangeOrderStatus::Draft {
            return Err(Error::from("只有草稿状态的采购变更单可以提交审批"));
        }
        let next = self
            .approval_subject_version
            .checked_add(1)
            .ok_or_else(|| Error::from("审批提交版本溢出"))?;
        let target_content_hash = normalize_required_text(
            target_content_hash.into(),
            "目标内容指纹不能为空",
            CONTENT_HASH_MAX_LEN,
            "目标内容指纹过长",
        )?;
        self.approval_subject_version = next;
        self.current_submission_id = Some(submission_id);
        self.target_content_hash = Some(target_content_hash);
        self.stable.status = PurchaseChangeOrderStatus::InApproval;
        self.stable.touch(updated_by);
        Ok(next)
    }

    /// 撤回审批：回到草稿，且 `approval_subject_version` 不回退。
    ///
    /// # 参数
    /// * `updated_by` - 撤回人
    ///
    /// # 错误
    /// 非审批中时返回冲突。
    pub fn cancel_approval(&mut self, updated_by: impl Into<String>) -> Result<()> {
        if self.stable.status != PurchaseChangeOrderStatus::InApproval {
            return Err(Error::from("只有审批中的采购变更单可以撤回审批"));
        }
        self.stable.status = PurchaseChangeOrderStatus::Draft;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 最终通过并生效：仅 `IN_APPROVAL` 可进入 `EFFECTIVE`。
    ///
    /// # 参数
    /// * `effective_revision_id` - 生效后形成的新采购版本
    /// * `updated_by` - 最终通过执行人
    ///
    /// # 错误
    /// 状态不是审批中时返回冲突。
    pub fn apply_effective(
        &mut self,
        effective_revision_id: PurchaseOrderRevisionId,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        if self.stable.status != PurchaseChangeOrderStatus::InApproval {
            return Err(Error::from("只有审批中的采购变更单可以由最终通过动作生效"));
        }
        self.effective_revision_id = Some(effective_revision_id);
        self.stable.status = PurchaseChangeOrderStatus::Effective;
        self.stable.touch(updated_by);
        Ok(())
    }
}

/// 判断更新数据是否包含内容字段。
///
/// # 参数
/// * `update` - 更新数据
///
/// # 返回
/// 包含原因/目标提交/内容指纹/生效版本任一字段时返回 `true`。
fn update_has_content(update: &PurchaseChangeOrderUpdate) -> bool {
    update.reason.is_some()
        || update.current_submission_id.is_some()
        || update.target_content_hash.is_some()
        || update.effective_revision_id.is_some()
}

/// 解析带固定前缀的十进制序号。
///
/// # 参数
/// * `value` - 完整编号
/// * `prefix` - 固定编号前缀
///
/// # 返回
/// 编号匹配前缀且后缀可解析为 `u32` 时返回序号，否则返回 `None`。
fn parse_sequence(value: &str, prefix: &str) -> Option<u32> {
    value.strip_prefix(prefix)?.parse().ok()
}

/// 规范化提交序号。
///
/// # 参数
/// * `submission_no` - 原始提交序号
///
/// # 返回
/// 返回去空白后的提交序号。
///
/// # 错误
/// 序号为空或超长时返回错误。
fn normalize_submission_no(submission_no: String) -> Result<String> {
    normalize_required_text(
        submission_no,
        "提交序号不能为空",
        SUBMISSION_NO_MAX_LEN,
        "提交序号过长",
    )
}

/// 采购变更提交创建数据（不含系统字段）。
///
/// 字段与 `purchase_order_submission` 相同，并增加
/// `purchase_change_order_id`、`submission_no`、`base_revision_id`（§6.6）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseChangeSubmissionData {
    /// 所属采购变更单。
    pub purchase_change_order_id: PurchaseChangeOrderId,
    /// 提交序号（聚合内唯一）。
    pub submission_no: String,
    /// 基准版本。
    pub base_revision_id: PurchaseOrderRevisionId,
    /// 供应商（拆单维度）。
    pub supplier_id: SupplierAccountId,
    /// 采购类型（拆单维度）。
    pub purchase_type: PurchaseType,
    /// 履约责任（拆单维度）。
    pub fulfillment_responsibility: FulfillmentResponsibility,
    /// 提交时供应商版本。
    pub supplier_revision_id: SupplierCommercialProfileRevisionId,
    /// 提交时供应商快照。
    pub supplier_snapshot: SupplierSnapshot,
    /// 付款条件和先款后货门禁快照。
    pub payment_term_snapshot: PaymentTermSnapshot,
    /// 含税行汇总。
    pub gross_amount: Amount,
    /// 不含税行汇总。
    pub net_amount: Amount,
    /// 税额行汇总。
    pub tax_amount: Amount,
}

/// 采购变更提交实体（不可变提交，数据模型 §6.6）。
///
/// 仓储影响确认与财务复核均引用该不可变提交；修改内容必须新建提交并使旧复核失效。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct PurchaseChangeSubmission {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属采购变更单。
    pub purchase_change_order_id: PurchaseChangeOrderId,
    /// 提交序号。
    pub submission_no: String,
    /// 基准版本。
    pub base_revision_id: PurchaseOrderRevisionId,
    /// 供应商。
    pub supplier_id: SupplierAccountId,
    /// 采购类型。
    pub purchase_type: PurchaseType,
    /// 履约责任。
    pub fulfillment_responsibility: FulfillmentResponsibility,
    /// 提交时供应商版本。
    pub supplier_revision_id: SupplierCommercialProfileRevisionId,
    /// 提交时供应商快照。
    pub supplier_snapshot: SupplierSnapshot,
    /// 付款条件和先款后货门禁快照。
    pub payment_term_snapshot: PaymentTermSnapshot,
    /// 含税行汇总。
    pub gross_amount: Amount,
    /// 不含税行汇总。
    pub net_amount: Amount,
    /// 税额行汇总。
    pub tax_amount: Amount,
    /// 提交状态（与采购提交同字典：草稿、待审核、已通过、已驳回、因重新提交失效）。
    pub status: SubmissionStatus,
    /// 提交审计时间；与 `submitted_by` 成对出现。
    pub submitted_at: Option<Instant>,
    /// 提交审计人；与 `submitted_at` 成对出现。
    pub submitted_by: Option<String>,
}

impl PurchaseChangeSubmission {
    /// 创建采购变更提交。
    ///
    /// 完成 `submission_no` 校验与规范化，并强制表头金额守恒
    /// （`gross = net + tax`，§4.2 铁律 4）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::PurchaseChangeSubmissionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的提交实体（初始状态 `Draft`）。
    ///
    /// # 错误
    /// 提交序号为空/超长，或表头金额三元组不守恒时返回错误。
    pub fn new(id: PurchaseChangeSubmissionId, data: PurchaseChangeSubmissionData) -> Result<Self> {
        let submission_no = normalize_submission_no(data.submission_no)?;
        ensure_header_triple(
            data.gross_amount,
            data.net_amount,
            data.tax_amount,
            &submission_no,
        )?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            purchase_change_order_id: data.purchase_change_order_id,
            submission_no,
            base_revision_id: data.base_revision_id,
            supplier_id: data.supplier_id,
            purchase_type: data.purchase_type,
            fulfillment_responsibility: data.fulfillment_responsibility,
            supplier_revision_id: data.supplier_revision_id,
            supplier_snapshot: data.supplier_snapshot,
            payment_term_snapshot: data.payment_term_snapshot,
            gross_amount: data.gross_amount,
            net_amount: data.net_amount,
            tax_amount: data.tax_amount,
            status: SubmissionStatus::Draft,
            submitted_at: None,
            submitted_by: None,
        })
    }

    /// 计算同一变更单的下一个提交序号。
    ///
    /// 仅识别 `CS-{n}` 形态的历史提交，忽略草稿或旧格式编号；新编号固定为
    /// 六位十进制序号。
    ///
    /// # 参数
    /// * `existing` - 同一采购变更单的既有提交
    ///
    /// # 返回
    /// 返回下一个 `CS-000001` 形态的提交序号。
    ///
    /// # 错误
    /// 最大合法序号已经达到 `u32::MAX` 时返回领域错误。
    pub fn next_submission_no(existing: &[Self]) -> Result<String> {
        let max_no = existing
            .iter()
            .filter_map(|submission| parse_sequence(&submission.submission_no, "CS-"))
            .max()
            .unwrap_or(0);
        let next = max_no
            .checked_add(1)
            .ok_or_else(|| Error::from("采购变更提交序号溢出"))?;
        Ok(format!("CS-{next:06}"))
    }

    /// 校验变更提交仍处于待处理状态。
    ///
    /// # 返回
    /// 待审核状态返回 `Ok(())`。
    ///
    /// # 错误
    /// 提交已经处理、失效或仍是草稿时返回领域错误。
    pub fn ensure_pending(&self) -> Result<()> {
        if self.status != SubmissionStatus::Pending {
            return Err(Error::from("变更提交已处理，请勿重复生效"));
        }
        Ok(())
    }

    /// 记录采购变更最终通过结论。
    ///
    /// # 返回
    /// 待审核提交成功改为已通过时返回 `Ok(())`。
    ///
    /// # 错误
    /// 提交不是待审核状态时返回领域错误。
    pub fn approve(&mut self) -> Result<()> {
        self.ensure_pending()?;
        self.status = SubmissionStatus::Approved;
        Ok(())
    }

    /// 提交复核。
    ///
    /// 从草稿进入待审核并写入提交审计；提交后头行冻结。
    ///
    /// # 参数
    /// * `submitted_at` - 提交时间
    /// * `submitted_by` - 提交人
    ///
    /// # 返回
    /// 提交成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态不是草稿时返回错误。
    pub fn submit(&mut self, submitted_at: Instant, submitted_by: impl Into<String>) -> Result<()> {
        if self.status != SubmissionStatus::Draft {
            return Err(Error::from("只有草稿状态的提交可以提交复核"));
        }
        self.status = SubmissionStatus::Pending;
        self.submitted_at = Some(submitted_at);
        self.submitted_by = Some(normalize_required_text(
            submitted_by.into(),
            "提交人不能为空",
            ACTOR_MAX_LEN,
            "提交人标识过长",
        )?);
        Ok(())
    }
}

/// 采购变更提交行创建数据（不含系统字段）。
///
/// 保存拟变更后的完整采购行及销售分配，字段与 `purchase_order_submission_line` 相同
/// （§6.6）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseChangeSubmissionLineData {
    /// 所属采购变更提交。
    pub purchase_change_submission_id: PurchaseChangeSubmissionId,
    /// 行号（从 1 递增）。
    pub line_no: u32,
    /// 行类型。
    pub line_type: PurchaseLineType,
    /// 商品/服务行对应的采购二次确认分行；物流费用行为空。
    pub procurement_confirmation_line_id: Option<ProcurementConfirmationLineId>,
    /// 商品行引用的 SKU；物流费用行为空。
    pub sku_id: Option<SkuId>,
    /// 商品行引用的 SKU 版本；物流费用行为空。
    pub sku_revision_id: Option<SkuRevisionId>,
    /// 商品名称快照；物流费用行为空。
    pub product_name_snapshot: Option<String>,
    /// 规格快照；物流费用行为空。
    pub specification_snapshot: Option<String>,
    /// 基础单位数量；物流费用行为空。
    pub quantity: Option<Quantity>,
    /// 单位代码；物流费用行为空。
    pub base_unit_code: Option<String>,
    /// 含税采购单价；物流费用行为空。
    pub unit_cost_gross: Option<UnitPrice>,
    /// 含税行金额。
    pub gross_amount: Amount,
    /// 不含税行金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 进项税率。
    pub input_tax_rate: Option<Rate>,
    /// 预计交期。
    pub expected_delivery_date: Option<BusinessDate>,
    /// 商品行对应的销售稳定行。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 商品行对应的销售当前版本行。
    pub sales_order_revision_line_id: Option<SalesOrderRevisionLineId>,
    /// 商品行对应的历史销售提交行；仅保留旧流程追溯。
    pub sales_order_submission_line_id: Option<SalesOrderSubmissionLineId>,
    /// 商品行对应的分配数量。
    pub allocated_quantity: Option<Quantity>,
}

/// 采购变更提交行实体（数据模型 §6.6）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct PurchaseChangeSubmissionLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属采购变更提交。
    pub purchase_change_submission_id: PurchaseChangeSubmissionId,
    /// 行号。
    pub line_no: u32,
    /// 行类型。
    pub line_type: PurchaseLineType,
    /// 商品/服务行对应的采购二次确认分行。
    pub procurement_confirmation_line_id: Option<ProcurementConfirmationLineId>,
    /// 商品行引用的 SKU。
    pub sku_id: Option<SkuId>,
    /// 商品行引用的 SKU 版本。
    pub sku_revision_id: Option<SkuRevisionId>,
    /// 商品名称快照。
    pub product_name_snapshot: Option<String>,
    /// 规格快照。
    pub specification_snapshot: Option<String>,
    /// 基础单位数量。
    pub quantity: Option<Quantity>,
    /// 单位代码。
    pub base_unit_code: Option<String>,
    /// 含税采购单价。
    pub unit_cost_gross: Option<UnitPrice>,
    /// 含税行金额。
    pub gross_amount: Amount,
    /// 不含税行金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 进项税率。
    pub input_tax_rate: Option<Rate>,
    /// 预计交期。
    pub expected_delivery_date: Option<BusinessDate>,
    /// 商品行对应的销售稳定行。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 商品行对应的销售当前版本行。
    pub sales_order_revision_line_id: Option<SalesOrderRevisionLineId>,
    /// 商品行对应的历史销售提交行；仅保留旧流程追溯。
    pub sales_order_submission_line_id: Option<SalesOrderSubmissionLineId>,
    /// 商品行对应的分配数量。
    pub allocated_quantity: Option<Quantity>,
}

impl PurchaseLineDataRef for PurchaseChangeSubmissionLineData {
    fn line_type(&self) -> PurchaseLineType {
        self.line_type
    }

    fn procurement_confirmation_line_id(&self) -> &Option<ProcurementConfirmationLineId> {
        &self.procurement_confirmation_line_id
    }

    fn sku_id(&self) -> &Option<SkuId> {
        &self.sku_id
    }

    fn product_name_snapshot(&self) -> &Option<String> {
        &self.product_name_snapshot
    }

    fn specification_snapshot(&self) -> &Option<String> {
        &self.specification_snapshot
    }

    fn quantity(&self) -> Option<Quantity> {
        self.quantity
    }

    fn base_unit_code(&self) -> &Option<String> {
        &self.base_unit_code
    }

    fn unit_cost_gross(&self) -> Option<UnitPrice> {
        self.unit_cost_gross
    }

    fn gross_amount(&self) -> Amount {
        self.gross_amount
    }

    fn net_amount(&self) -> Amount {
        self.net_amount
    }

    fn tax_amount(&self) -> Amount {
        self.tax_amount
    }

    fn input_tax_rate(&self) -> Option<Rate> {
        self.input_tax_rate
    }

    fn ensure_allocation(&self) -> Result<()> {
        match self.line_type {
            PurchaseLineType::ItemService => {
                if self.sales_order_line_id.is_none() || self.sales_order_revision_line_id.is_none() {
                    return Err(Error::from("商品/服务行必须引用销售稳定行与当前版本行"));
                }
                let quantity = self.allocated_quantity.ok_or("商品/服务行必须填写分配数量")?;
                if quantity.to_decimal() <= rust_decimal::Decimal::ZERO {
                    return Err(Error::from("商品/服务行分配数量必须为正"));
                }
            }
            PurchaseLineType::LogisticsFee => {
                if self.sales_order_line_id.is_some()
                    || self.sales_order_revision_line_id.is_some()
                    || self.sales_order_submission_line_id.is_some()
                    || self.allocated_quantity.is_some()
                {
                    return Err(Error::from("物流费用行不得携带销售分配"));
                }
            }
        }
        Ok(())
    }
}

impl PurchaseChangeSubmissionLine {
    /// 创建采购变更提交行。
    ///
    /// 完成快照文本的规范化，并按行类型强制字段归属与金额三元组守恒（§6.6）；
    /// 商品行必须携带销售提交行引用与分配数量。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::PurchaseChangeSubmissionLineId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的提交行实体。
    ///
    /// # 错误
    /// 行号为零、字段归属与行类型不符、快照超长、数量/单价/税率越界或
    /// 金额三元组不守恒时返回错误。
    pub fn new(id: PurchaseChangeSubmissionLineId, data: PurchaseChangeSubmissionLineData) -> Result<Self> {
        ensure_line_no(data.line_no)?;
        let (product_name, specification, base_unit_code) = normalize_and_validate_line(&data)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            purchase_change_submission_id: data.purchase_change_submission_id,
            line_no: data.line_no,
            line_type: data.line_type,
            procurement_confirmation_line_id: data.procurement_confirmation_line_id,
            sku_id: data.sku_id.clone(),
            sku_revision_id: data.sku_revision_id,
            product_name_snapshot: product_name,
            specification_snapshot: specification,
            quantity: data.quantity,
            base_unit_code,
            unit_cost_gross: data.unit_cost_gross,
            gross_amount: data.gross_amount,
            net_amount: data.net_amount,
            tax_amount: data.tax_amount,
            input_tax_rate: data.input_tax_rate,
            expected_delivery_date: data.expected_delivery_date,
            sales_order_line_id: data.sales_order_line_id,
            sales_order_revision_line_id: data.sales_order_revision_line_id,
            sales_order_submission_line_id: data.sales_order_submission_line_id,
            allocated_quantity: data.allocated_quantity,
        })
    }
}

/// 校验行号从 1 开始。
///
/// # 参数
/// * `line_no` - 行号
///
/// # 错误
/// 行号为零时返回错误。
fn ensure_line_no(line_no: u32) -> Result<()> {
    if line_no == 0 {
        return Err(Error::from("行号必须从 1 开始"));
    }
    Ok(())
}

/// 校验表头金额三元组守恒。
///
/// # 参数
/// * `gross_amount` / `net_amount` / `tax_amount` - 表头汇总
/// * `context` - 错误提示中的上下文（如提交序号）
///
/// # 错误
/// `gross ≠ net + tax` 或任一分量为负时返回错误。
fn ensure_header_triple(
    gross_amount: Amount,
    net_amount: Amount,
    tax_amount: Amount,
    context: &str,
) -> Result<()> {
    if gross_amount.to_decimal() != net_amount.to_decimal() + tax_amount.to_decimal()
        || gross_amount.to_decimal() < rust_decimal::Decimal::ZERO
        || net_amount.to_decimal() < rust_decimal::Decimal::ZERO
        || tax_amount.to_decimal() < rust_decimal::Decimal::ZERO
    {
        return Err(Error::from(format!("变更提交表头金额三元组不守恒（{context}）")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        PurchaseChangeOrder, PurchaseChangeOrderData, PurchaseChangeOrderStatus, PurchaseChangeOrderUpdate,
        PurchaseChangeSubmission, PurchaseChangeSubmissionData, PurchaseChangeSubmissionLine,
        PurchaseChangeSubmissionLineData,
    };
    use crate::common::time::{BusinessDate, Instant};
    use crate::ids::{
        ProcurementConfirmationLineId, PurchaseChangeOrderId, PurchaseChangeSubmissionId,
        PurchaseChangeSubmissionLineId, PurchaseOrderId, PurchaseOrderRevisionId, SalesOrderLineId,
        SalesOrderRevisionLineId, SalesOrderSubmissionLineId, SkuId, SupplierAccountId,
        SupplierCommercialProfileRevisionId,
    };
    use crate::money::{line_amounts, Amount, Quantity, Rate, UnitPrice};
    use crate::purchase_order::purchase_submission::SubmissionStatus;
    use crate::purchase_order::snapshot::{PaymentTermSnapshot, SupplierSnapshot};
    use crate::purchase_order::types::{FulfillmentResponsibility, PurchaseLineType, PurchaseType};
    use std::str::FromStr;

    fn snapshot() -> SupplierSnapshot {
        SupplierSnapshot::new("北京华联供应商".to_string()).unwrap()
    }

    fn payment_term() -> PaymentTermSnapshot {
        PaymentTermSnapshot::new("NET-30".to_string(), false, None, None).unwrap()
    }

    fn change_data() -> PurchaseChangeOrderData {
        PurchaseChangeOrderData {
            purchase_order_id: PurchaseOrderId::new("po-1"),
            base_revision_id: PurchaseOrderRevisionId::new("por-1"),
            reason: " 成本上涨调整 ".to_string(),
        }
    }

    fn change_submission_data() -> PurchaseChangeSubmissionData {
        PurchaseChangeSubmissionData {
            purchase_change_order_id: PurchaseChangeOrderId::new("pco-1"),
            submission_no: "CS-01".to_string(),
            base_revision_id: PurchaseOrderRevisionId::new("por-1"),
            supplier_id: SupplierAccountId::new("sup-1"),
            purchase_type: PurchaseType::Physical,
            fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
            supplier_revision_id: SupplierCommercialProfileRevisionId::new("spr-1"),
            supplier_snapshot: snapshot(),
            payment_term_snapshot: payment_term(),
            gross_amount: Amount::from_str("29.97").unwrap(),
            net_amount: Amount::from_str("26.07").unwrap(),
            tax_amount: Amount::from_str("3.90").unwrap(),
        }
    }

    fn change_line_data() -> PurchaseChangeSubmissionLineData {
        let (gross, net, tax) = line_amounts(
            UnitPrice::from_str("9.9900").unwrap(),
            Quantity::from_str("3.000000").unwrap(),
            Rate::from_str("0.130000").unwrap(),
        );
        PurchaseChangeSubmissionLineData {
            purchase_change_submission_id: PurchaseChangeSubmissionId::new("pcs-1"),
            line_no: 1,
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new("pcl-1")),
            sku_id: Some(SkuId::new("sku-1")),
            sku_revision_id: Some(crate::ids::SkuRevisionId::new("skur-1")),
            product_name_snapshot: Some("慰问礼包".to_string()),
            specification_snapshot: Some("500g×2".to_string()),
            quantity: Some(Quantity::from_str("3.000000").unwrap()),
            base_unit_code: Some("箱".to_string()),
            unit_cost_gross: Some(UnitPrice::from_str("9.9900").unwrap()),
            gross_amount: gross,
            net_amount: net,
            tax_amount: tax,
            input_tax_rate: Some(Rate::from_str("0.130000").unwrap()),
            expected_delivery_date: Some(BusinessDate::from_ymd(2026, 8, 6).unwrap()),
            sales_order_line_id: Some(SalesOrderLineId::new("sol-1")),
            sales_order_revision_line_id: Some(SalesOrderRevisionLineId::new("sorl-1")),
            sales_order_submission_line_id: Some(SalesOrderSubmissionLineId::new("ssl-1")),
            allocated_quantity: Some(Quantity::from_str("3.000000").unwrap()),
        }
    }

    #[test]
    fn change_order_new_trims_reason_and_starts_draft() {
        let order =
            PurchaseChangeOrder::new(PurchaseChangeOrderId::new("pco-1"), change_data(), "admin-1").unwrap();
        assert_eq!(order.reason, "成本上涨调整");
        assert_eq!(order.stable.status(), PurchaseChangeOrderStatus::Draft);
        assert_eq!(order.approval_subject_version, 0);
        assert!(order.current_submission_id.is_none());
    }

    #[test]
    fn change_order_version_submission_and_base_revision_guards_are_owned_by_entity() {
        let mut order =
            PurchaseChangeOrder::new(PurchaseChangeOrderId::new("pco-1"), change_data(), "admin-1").unwrap();
        order.ensure_expected_version(order.base.version).unwrap();
        assert!(order
            .ensure_expected_version(order.base.version.saturating_add(1))
            .is_err());
        order.ensure_draft_for_submission().unwrap();
        assert!(order.submission_id_for_effect(None).is_err());
        order
            .start_approval(PurchaseChangeSubmissionId::new("pcs-1"), "hash-1", "admin-1")
            .unwrap();
        assert_eq!(
            order.submission_id_for_effect(Some("pcs-1")).unwrap().as_ref(),
            "pcs-1"
        );
        assert!(order.submission_id_for_effect(Some("pcs-old")).is_err());
        order.ensure_base_revision_current(Some("por-1")).unwrap();
        assert!(order.ensure_base_revision_current(Some("por-2")).is_err());
    }

    #[test]
    fn change_order_update_gates_content_on_draft() {
        let mut order =
            PurchaseChangeOrder::new(PurchaseChangeOrderId::new("pco-1"), change_data(), "admin-1").unwrap();
        order
            .update(
                PurchaseChangeOrderUpdate {
                    reason: Some("价格下降".to_string()),
                    current_submission_id: Some(PurchaseChangeSubmissionId::new("pcs-1")),
                    target_content_hash: Some("hash-1".to_string()),
                    ..Default::default()
                },
                "admin-2",
            )
            .unwrap();
        assert_eq!(order.reason, "价格下降");
        assert_eq!(order.stable.updated_by, "admin-2");

        order
            .start_approval(PurchaseChangeSubmissionId::new("pcs-1"), "hash-1", "admin-2")
            .unwrap();
        assert_eq!(order.stable.status(), PurchaseChangeOrderStatus::InApproval);
        assert_eq!(order.approval_subject_version, 1);

        assert!(
            order
                .update(
                    PurchaseChangeOrderUpdate {
                        reason: Some("再改".to_string()),
                        ..Default::default()
                    },
                    "admin-3",
                )
                .is_err(),
            "非草稿不得编辑内容"
        );
    }

    /// 提交进入审批中；撤回不回退版本；最终通过进入生效。
    #[test]
    fn start_approval_cancel_and_apply_effective() {
        let mut order =
            PurchaseChangeOrder::new(PurchaseChangeOrderId::new("pco-1"), change_data(), "admin-1").unwrap();
        let version = order
            .start_approval(PurchaseChangeSubmissionId::new("pcs-1"), "hash-1", "submitter-1")
            .unwrap();
        assert_eq!(version, 1);
        assert_eq!(order.stable.status(), PurchaseChangeOrderStatus::InApproval);
        assert_eq!(order.approval_subject_version, 1);
        assert_eq!(order.stable.updated_by, "submitter-1");

        order.cancel_approval("admin-2").unwrap();
        assert_eq!(order.stable.status(), PurchaseChangeOrderStatus::Draft);
        assert_eq!(order.approval_subject_version, 1);
        assert_eq!(
            order
                .current_submission_id
                .as_ref()
                .map(ToString::to_string)
                .as_deref(),
            Some("pcs-1")
        );

        let next = order
            .start_approval(PurchaseChangeSubmissionId::new("pcs-2"), "hash-2", "submitter-2")
            .unwrap();
        assert_eq!(next, 2);
        order
            .apply_effective(PurchaseOrderRevisionId::new("por-2"), "approver-1")
            .unwrap();
        assert_eq!(order.stable.status(), PurchaseChangeOrderStatus::Effective);
        assert_eq!(order.approval_subject_version, 2);
        assert!(order
            .start_approval(PurchaseChangeSubmissionId::new("pcs-3"), "h", "u")
            .is_err());
        assert!(order.cancel_approval("u").is_err());
        assert!(order
            .apply_effective(PurchaseOrderRevisionId::new("por-3"), "u")
            .is_err());
    }

    /// 内容更新不得改写状态；状态只能经签署邻接方法迁移。
    #[test]
    fn update_cannot_rewrite_status() {
        let mut order =
            PurchaseChangeOrder::new(PurchaseChangeOrderId::new("pco-1"), change_data(), "admin-1").unwrap();
        order
            .update(
                PurchaseChangeOrderUpdate {
                    reason: Some("仅改原因".to_string()),
                    ..Default::default()
                },
                "admin-2",
            )
            .unwrap();
        assert_eq!(order.stable.status(), PurchaseChangeOrderStatus::Draft);
        assert_eq!(order.reason, "仅改原因");
    }

    #[test]
    fn change_order_new_rejects_empty_reason() {
        let data = PurchaseChangeOrderData {
            reason: "   ".to_string(),
            ..change_data()
        };
        assert!(PurchaseChangeOrder::new(PurchaseChangeOrderId::new("pco-2"), data, "admin-1").is_err());
    }

    #[test]
    fn change_submission_validates_triple_and_submits() {
        let submission =
            PurchaseChangeSubmission::new(PurchaseChangeSubmissionId::new("pcs-1"), change_submission_data())
                .unwrap();
        assert_eq!(submission.status, SubmissionStatus::Draft);

        let inconsistent = PurchaseChangeSubmissionData {
            gross_amount: Amount::from_str("30.00").unwrap(),
            ..change_submission_data()
        };
        assert!(
            PurchaseChangeSubmission::new(PurchaseChangeSubmissionId::new("pcs-2"), inconsistent).is_err()
        );

        let mut pending =
            PurchaseChangeSubmission::new(PurchaseChangeSubmissionId::new("pcs-3"), change_submission_data())
                .unwrap();
        pending
            .submit(Instant::from_unix_secs(1_700_000_000), "buyer-1")
            .unwrap();
        assert_eq!(pending.status, SubmissionStatus::Pending);
        assert!(pending
            .submit(Instant::from_unix_secs(1_700_000_000), "buyer-1")
            .is_err());
    }

    #[test]
    fn change_submission_derives_next_number_and_approves_only_pending() {
        let mut first =
            PurchaseChangeSubmission::new(PurchaseChangeSubmissionId::new("pcs-1"), change_submission_data())
                .unwrap();
        first.submission_no = "CS-000009".to_string();
        assert_eq!(
            PurchaseChangeSubmission::next_submission_no(std::slice::from_ref(&first)).unwrap(),
            "CS-000010"
        );
        assert!(first.approve().is_err());
        first
            .submit(Instant::from_unix_secs(1_700_000_000), "buyer-1")
            .unwrap();
        first.approve().unwrap();
        assert_eq!(first.status, SubmissionStatus::Approved);
        assert!(first.approve().is_err());
    }

    #[test]
    fn change_submission_line_happy_and_failure() {
        let line = PurchaseChangeSubmissionLine::new(
            PurchaseChangeSubmissionLineId::new("pcsl-1"),
            change_line_data(),
        )
        .unwrap();
        assert_eq!(line.line_type, PurchaseLineType::ItemService);

        let bad_amounts = PurchaseChangeSubmissionLineData {
            gross_amount: Amount::from_str("29.98").unwrap(),
            ..change_line_data()
        };
        assert!(PurchaseChangeSubmissionLine::new(
            PurchaseChangeSubmissionLineId::new("pcsl-2"),
            bad_amounts,
        )
        .is_err());

        let fee_with_quantity = PurchaseChangeSubmissionLineData {
            line_type: PurchaseLineType::LogisticsFee,
            quantity: Some(Quantity::from_str("3.000000").unwrap()),
            ..change_line_data()
        };
        assert!(PurchaseChangeSubmissionLine::new(
            PurchaseChangeSubmissionLineId::new("pcsl-3"),
            fee_with_quantity,
        )
        .is_err());
    }
}
