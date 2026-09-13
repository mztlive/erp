use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use erp_core::common::stable::StableBase;
use erp_core::common::state::ensure_transition;
use erp_core::common::time::Instant;
use erp_core::ids::{ContractId, CustomerAccountId, PartyId, SalesOrderId, SalesOrderRevisionId};
use erp_core::money::Quantity;
use erp_core::validation::{normalize_optional_text, normalize_required_text};
use erp_core::{Error, Result};

use super::super::types::{BusinessType, OriginSystem};
use super::{
    CloseStatus, CollectionProgress, CommercialStatus, FulfillmentProgress, InvoiceProgress, ReviewStatus,
};

/// 销售单号最大长度。
const ORDER_NO_MAX_LEN: usize = 64;
/// 一期来源身份引用最大长度。
const SOURCE_IDENTITY_MAX_LEN: usize = 256;
/// 来源状态代码最大长度。
const SOURCE_STATUS_CODE_MAX_LEN: usize = 64;

/// 销售单创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderData {
    /// 创建时显式确定的内部业务组织。
    pub business_org_unit_id: String,
    /// 明确的内部销售责任身份；创建用例负责校验来源映射，禁止以系统执行人兜底。
    pub sales_owner_user_id: String,
    /// 销售单号（唯一，创建后不可修改）。
    pub order_no: String,
    /// 业务性质（创建后永久不变）。
    pub business_type: BusinessType,
    /// 最初创建入口（创建后永久不变）。
    pub origin_system: OriginSystem,
    /// 一期商城来源键映射；ERP 新建单为空。
    pub source_identity_id: Option<String>,
    /// 客户稳定身份。
    pub customer_id: CustomerAccountId,
    /// 合同稳定身份（无合同时为空）。
    pub contract_id: Option<ContractId>,
    /// 结算主体。
    pub settlement_party_id: PartyId,
    /// 一期商城原始状态代码，只用于追溯。
    pub source_status_code: Option<String>,
}

/// 销售单更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SalesOrderUpdate {
    /// 客户稳定身份；`None` 表示不修改。
    pub customer_id: Option<CustomerAccountId>,
    /// 合同稳定身份；`None` 表示不修改。
    pub contract_id: Option<ContractId>,
    /// 结算主体；`None` 表示不修改。
    pub settlement_party_id: Option<PartyId>,
    /// 来源状态代码；`None` 表示不修改。
    pub source_status_code: Option<String>,
}

/// 销售单实体（稳定主表，数据模型 §6.4）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）以替代约定中的派生写法。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct SalesOrder {
    /// 单据业务组织不随负责人调岗变化。
    pub business_org_unit_id: String,
    /// 仅首次生效时冻结，普通编辑不可修改。
    pub attribution: Option<super::super::SalesAttribution>,
    /// 单据负责销售；ERP 创建时取认证建单人，普通编辑不得修改。
    pub sales_owner_user_id: String,
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<CommercialStatus>,
    /// 销售单号（创建后不可修改）。
    pub order_no: String,
    /// 业务性质（创建后永久不变）。
    pub business_type: BusinessType,
    /// 最初创建入口（创建后永久不变）。
    pub origin_system: OriginSystem,
    /// 一期商城来源键映射。
    pub source_identity_id: Option<String>,
    /// 客户稳定身份。
    pub customer_id: CustomerAccountId,
    /// 合同稳定身份。
    pub contract_id: Option<ContractId>,
    /// 结算主体。
    pub settlement_party_id: PartyId,
    /// 商业主状态（§7.1 仅 4 值）。
    pub commercial_status: CommercialStatus,
    /// 审核轨状态（§7.1 审核环节值，禁止写回主状态）。
    pub review_status: ReviewStatus,
    /// 履约进度。
    pub fulfillment_progress: FulfillmentProgress,
    /// 回款进度。
    pub collection_progress: CollectionProgress,
    /// 开票进度。
    pub invoice_progress: InvoiceProgress,
    /// 关闭状态。
    pub close_status: CloseStatus,
    /// 一期商城原始状态，只用于追溯。
    pub source_status_code: Option<String>,
    /// 生效时间。
    pub effective_at: Option<Instant>,
    /// ERP 关闭时间。
    pub closed_at: Option<Instant>,
    /// 采购创建串行化版本；每次成功占用采购剩余量前递增。
    #[serde(default)]
    pub procurement_guard_version: u64,
}

impl PartialEq for SalesOrder {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.sales_owner_user_id == other.sales_owner_user_id
            && self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.order_no == other.order_no
            && self.business_type == other.business_type
            && self.origin_system == other.origin_system
            && self.source_identity_id == other.source_identity_id
            && self.customer_id == other.customer_id
            && self.contract_id == other.contract_id
            && self.settlement_party_id == other.settlement_party_id
            && self.commercial_status == other.commercial_status
            && self.review_status == other.review_status
            && self.fulfillment_progress == other.fulfillment_progress
            && self.collection_progress == other.collection_progress
            && self.invoice_progress == other.invoice_progress
            && self.close_status == other.close_status
            && self.source_status_code == other.source_status_code
            && self.effective_at == other.effective_at
            && self.business_org_unit_id == other.business_org_unit_id
            && self.attribution == other.attribution
            && self.closed_at == other.closed_at
            && self.procurement_guard_version == other.procurement_guard_version
    }
}

impl Eq for SalesOrder {}

impl SalesOrder {
    /// 创建销售单（草稿态；明细与正式内容在销售审批提交时形成）。
    ///
    /// 完成 order_no 等文本字段的校验与规范化（去首尾空白、非空、长度上限），
    /// 一期来源单 `source_identity_id`/`source_status_code` 只作追溯不参与业务判定。
    ///
    /// # 参数
    /// * `id` - 实体主键（`erp_core::ids::SalesOrderId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的销售单实体（`Draft`、`NotSubmitted`）。
    ///
    /// # 错误
    /// 当 order_no 为空/超长或可选字段超长时返回错误。
    pub fn new(id: SalesOrderId, data: SalesOrderData, created_by: impl Into<String>) -> Result<Self> {
        let order_no = normalize_required_text(
            data.order_no,
            "销售单号不能为空",
            ORDER_NO_MAX_LEN,
            "销售单号过长",
        )?;
        let source_identity_id =
            normalize_optional_text(data.source_identity_id, "来源身份引用", SOURCE_IDENTITY_MAX_LEN)?;
        let source_status_code = normalize_optional_text(
            data.source_status_code,
            "来源状态代码",
            SOURCE_STATUS_CODE_MAX_LEN,
        )?;

        let created_by = created_by.into();
        let sales_owner_user_id = normalize_required_text(
            data.sales_owner_user_id,
            "负责销售不能为空",
            128,
            "负责销售 ID 过长",
        )?;
        Ok(Self {
            sales_owner_user_id,
            business_org_unit_id: normalize_required_text(
                data.business_org_unit_id,
                "业务组织不能为空",
                128,
                "业务组织身份过长",
            )?,
            attribution: None,
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(CommercialStatus::Draft, created_by),
            order_no,
            business_type: data.business_type,
            origin_system: data.origin_system,
            source_identity_id,
            customer_id: data.customer_id,
            contract_id: data.contract_id,
            settlement_party_id: data.settlement_party_id,
            commercial_status: CommercialStatus::Draft,
            review_status: ReviewStatus::NotSubmitted,
            fulfillment_progress: FulfillmentProgress::NotStarted,
            collection_progress: CollectionProgress::NotCollected,
            invoice_progress: InvoiceProgress::NotInvoiced,
            close_status: CloseStatus::NotSatisfied,
            source_status_code,
            effective_at: None,
            closed_at: None,
            procurement_guard_version: 0,
        })
    }

    /// 判断乐观锁版本是否与调用方期望一致。
    ///
    /// # 参数
    /// * `expected_version` - 调用方读取到的实体版本
    ///
    /// # 返回
    /// 当前版本与期望版本一致时返回 `true`。
    pub fn matches_version(&self, expected_version: u64) -> bool {
        self.base.version == expected_version
    }

    /// 判断销售单是否已经完整形式化。
    ///
    /// # 返回
    /// 商务状态为已生效且审核轨为已通过时返回 `true`。
    pub fn is_fully_formalized(&self) -> bool {
        self.commercial_status == CommercialStatus::Effective && self.review_status == ReviewStatus::Approved
    }

    /// 判断销售单是否仍绑定给定合同、客户与结算主体。
    ///
    /// # 参数
    /// * `contract_id` - 当前命令选择的合同
    /// * `customer_id` - 合同解析出的客户
    /// * `settlement_party_id` - 合同解析出的结算主体
    ///
    /// # 返回
    /// 三项关系与销售单稳定关系完全一致时返回 `true`。
    pub fn matches_contract_context(
        &self,
        contract_id: &ContractId,
        customer_id: &CustomerAccountId,
        settlement_party_id: &PartyId,
    ) -> bool {
        self.contract_id.as_ref() == Some(contract_id)
            && &self.customer_id == customer_id
            && &self.settlement_party_id == settlement_party_id
    }

    /// 返回发起销售变更前的实体内阻塞原因。
    ///
    /// # 返回
    /// 非生效、缺当前版本或卡券销售时返回稳定阻塞说明；实体状态允许时返回 `None`。
    pub fn sales_change_start_blocker(&self) -> Option<&'static str> {
        if self.commercial_status != CommercialStatus::Effective {
            return Some("只有已生效的销售单才能发起变更");
        }
        if self.stable.current_revision_id.is_none() {
            return Some("销售单缺少当前版本，无法发起变更");
        }
        self.business_type
            .is_voucher()
            .then_some("卡券销售变更缺少原正式版本冻结的目标商城或应收到期日，禁止创建变更单")
    }

    /// 返回当前生效版本身份。
    ///
    /// # 返回
    /// 已形成正式版本时返回版本 ID 字符串；草稿尚无版本时返回 `None`。
    pub fn current_revision_id(&self) -> Option<&str> {
        self.stable.current_revision_id.as_deref()
    }

    /// 判断当前版本指针是否仍指向给定基准版本。
    ///
    /// # 参数
    /// * `base_revision_id` - 销售变更发起时冻结的基准版本
    ///
    /// # 返回
    /// 当前版本与基准版本一致时返回 `true`。
    pub fn current_revision_matches(&self, base_revision_id: &SalesOrderRevisionId) -> bool {
        self.stable.current_revision_id.as_deref() == Some(base_revision_id.as_ref())
    }

    /// 返回供给分配前的实体内阻塞原因。
    ///
    /// # 参数
    /// * `remaining_quantity` - 当前销售版本尚未被供给覆盖的数量
    ///
    /// # 返回
    /// 非实物服务、未生效或无剩余数量时返回稳定阻塞说明；实体状态允许时返回
    /// `None`。权限与责任任务仍由 Service 校验。
    pub fn procurement_creation_blocker(&self, remaining_quantity: Quantity) -> Option<&'static str> {
        if !self.business_type.is_goods_service() {
            return Some("非实物及服务销售单无需供给分配");
        }
        if self.commercial_status != CommercialStatus::Effective {
            return Some("销售单最终生效后才能分配供给");
        }
        (remaining_quantity.to_decimal() <= rust_decimal::Decimal::ZERO)
            .then_some("当前销售单待分配供给已全部覆盖")
    }

    /// 刷新履约、回款、开票与关闭进度。
    ///
    /// # 参数
    /// * `fulfillment` - 可选外部履约进度；为空时沿用当前值
    /// * `collection` - 从应收子账派生的回款进度
    /// * `invoice` - 从应收子账派生的开票进度
    /// * `changed_at` - 首次满足自动关闭条件的时间
    /// * `updated_by` - 触发刷新的人或系统身份
    ///
    /// # 返回
    /// 任一进度发生变化并已更新实体时返回 `true`；无变化返回 `false`。
    pub fn refresh_progress(
        &mut self,
        fulfillment: Option<FulfillmentProgress>,
        collection: CollectionProgress,
        invoice: InvoiceProgress,
        changed_at: Instant,
        updated_by: impl Into<String>,
    ) -> bool {
        let next_fulfillment = fulfillment.unwrap_or(self.fulfillment_progress);
        let close = CloseStatus::from_progress(next_fulfillment, collection);
        if self.fulfillment_progress == next_fulfillment
            && self.collection_progress == collection
            && self.invoice_progress == invoice
            && self.close_status == close
        {
            return false;
        }
        if close == CloseStatus::Closed && self.close_status != CloseStatus::Closed {
            self.closed_at = Some(changed_at);
        }
        self.fulfillment_progress = next_fulfillment;
        self.collection_progress = collection;
        self.invoice_progress = invoice;
        self.close_status = close;
        self.stable.touch(updated_by);
        true
    }

    /// 递增采购创建串行化版本。
    ///
    /// # 参数
    /// * `updated_by` - 本次采购占用命令的执行人
    ///
    /// # 返回
    /// 返回递增后的采购串行化版本。
    ///
    /// # 错误
    /// 版本达到 `u64::MAX` 时返回领域错误。
    ///
    /// # 关键业务约束
    /// 调用方必须在 MongoDB 事务内通过销售单乐观锁写回，再重算采购剩余数量。
    pub fn advance_procurement_guard(&mut self, updated_by: impl Into<String>) -> Result<u64> {
        self.procurement_guard_version = self
            .procurement_guard_version
            .checked_add(1)
            .ok_or_else(|| Error::from("采购串行化版本溢出"))?;
        self.stable.touch(updated_by);
        Ok(self.procurement_guard_version)
    }

    /// 更新销售单。
    ///
    /// 复用 `new` 的校验规则；`order_no`/`business_type`/`origin_system` 是关键
    /// 身份字段，不允许在通用更新中修改（§6.4 约束）。`EFFECTIVE` 后不可直接编辑
    /// （§7.1：变化通过 `sales_change_order`），`VOIDED` 后同样锁定。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 已生效/已作废，或更新字段校验失败时返回错误。
    pub fn update(&mut self, update: SalesOrderUpdate, updated_by: impl Into<String>) -> Result<()> {
        self.ensure_editable()?;
        if let Some(customer_id) = update.customer_id {
            self.customer_id = customer_id;
        }
        if let Some(contract_id) = update.contract_id {
            self.contract_id = Some(contract_id);
        }
        if let Some(settlement_party_id) = update.settlement_party_id {
            self.settlement_party_id = settlement_party_id;
        }
        if let Some(source_status_code) = update.source_status_code {
            self.source_status_code = normalize_optional_text(
                Some(source_status_code),
                "来源状态代码",
                SOURCE_STATUS_CODE_MAX_LEN,
            )?;
        }
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 提交进入审核（主状态 `DRAFT → PENDING_REVIEW`）。
    ///
    /// 实物及服务与卡券目标路径均进入 `IN_APPROVAL`。旧逐节点复核态不得由本方法写入。
    ///
    /// # 参数
    /// * `updated_by` - 提交人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非草稿态或审核轨非未提交态时返回 [`Error::InvalidStateTransition`]。
    pub fn submit_for_review(&mut self, updated_by: impl Into<String>) -> Result<()> {
        ensure_transition(self.commercial_status, CommercialStatus::PendingReview)?;
        ensure_transition(self.review_status, ReviewStatus::NotSubmitted)?;
        self.transition_commercial_status(CommercialStatus::PendingReview)?;
        self.review_status = ReviewStatus::InApproval;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 销售单提交并进入统一审批。
    ///
    /// 商业主状态变为 `PENDING_REVIEW`，审核轨只允许 `IN_APPROVAL`。
    /// `GoodsService` 与 `Voucher` 共用本端口，不得再写入卡券两级复核态。
    ///
    /// # 参数
    /// * `updated_by` - 提交人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非草稿或审核轨非未提交时返回冲突或非法迁移。
    pub fn start_approval_submission(&mut self, updated_by: impl Into<String>) -> Result<()> {
        self.submit_for_review(updated_by)
    }

    /// 撤回统一审批提交，回到可修正草稿。
    ///
    /// 只允许从 `IN_APPROVAL` 撤回；`subject_version` 不回退。
    /// 不得经 `REJECTED` 或逐节点复核态。
    ///
    /// # 参数
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 审核轨不是审批中时返回冲突。
    pub fn cancel_approval_submission(&mut self, updated_by: impl Into<String>) -> Result<()> {
        if self.review_status != ReviewStatus::InApproval {
            return Err(Error::from("只有审批中的销售单可以撤回审批提交"));
        }
        self.return_to_draft(updated_by)
    }

    /// 驳回后回到可处理草稿（主状态 `PENDING_REVIEW → DRAFT`，§7.1）。
    ///
    /// # 参数
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非审核中态时返回 [`Error::InvalidStateTransition`]。
    pub fn return_to_draft(&mut self, updated_by: impl Into<String>) -> Result<()> {
        self.transition_commercial_status(CommercialStatus::Draft)?;
        self.review_status = ReviewStatus::NotSubmitted;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 审批通过并生效（主状态 `PENDING_REVIEW → EFFECTIVE`）。
    ///
    /// 审核轨同时推进到 `Approved`。目标路径只接受 `IN_APPROVAL`。
    /// 生效时间由调用方以服务端时间给出。
    ///
    /// # 参数
    /// * `effective_at` - 生效时间
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非审核中态或审核轨不允许直接通过时返回
    /// [`Error::InvalidStateTransition`]。
    pub fn approve(&mut self, effective_at: Instant, updated_by: impl Into<String>) -> Result<()> {
        if self
            .attribution
            .as_ref()
            .is_some_and(|snapshot| snapshot.attributed_at != effective_at)
        {
            return Err(Error::from("归属时点必须与首次生效时点一致"));
        }
        self.attribution
            .as_ref()
            .ok_or_else(|| Error::from("首次生效归属快照缺失"))?
            .validate(&self.sales_owner_user_id, &self.business_org_unit_id)?;
        ensure_transition(self.commercial_status, CommercialStatus::Effective)?;
        ensure_transition(self.review_status, ReviewStatus::Approved)?;
        self.transition_commercial_status(CommercialStatus::Effective)?;
        self.review_status = ReviewStatus::Approved;
        self.effective_at = Some(effective_at);
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 冻结首次生效归属；已有快照不可覆盖。
    ///
    /// # 错误
    /// 责任不一致、快照不完整或已经冻结时拒绝。
    pub fn freeze_attribution(&mut self, snapshot: super::super::SalesAttribution) -> Result<()> {
        if self.commercial_status != CommercialStatus::PendingReview {
            return Err(Error::from("仅审核中的销售单可准备首次生效归属"));
        }
        if self.attribution.is_some() || self.effective_at.is_some() {
            return Err(Error::from("首次生效归属不得重复冻结"));
        }
        snapshot.validate(&self.sales_owner_user_id, &self.business_org_unit_id)?;
        self.attribution = Some(snapshot);
        Ok(())
    }

    /// 作废草稿（主状态 `DRAFT → VOIDED`）。
    ///
    /// # 参数
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非草稿态时返回 [`Error::InvalidStateTransition`]。
    pub fn void(&mut self, updated_by: impl Into<String>) -> Result<()> {
        self.transition_commercial_status(CommercialStatus::Voided)?;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 推进唯一商业主状态并保持 `StableBase` 的通用状态镜像一致。
    ///
    /// 业务查询和索引只使用 `commercial_status`；`StableBase.status` 仅是实体基元
    /// 随带的同类型镜像，任何状态迁移都必须在本方法内原子同步，禁止形成两个
    /// 不同的销售主状态。
    fn transition_commercial_status(&mut self, to: CommercialStatus) -> Result<()> {
        ensure_transition(self.commercial_status, to)?;
        self.commercial_status = to;
        self.stable.status = to;
        Ok(())
    }

    /// 推进审核轨状态（不改变主状态）。
    ///
    /// # 参数
    /// * `to` - 目标审核轨状态
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 主状态非审核中，或迁移非法时返回 [`Error::InvalidStateTransition`]。
    pub fn transition_review(&mut self, to: ReviewStatus, updated_by: impl Into<String>) -> Result<()> {
        if self.commercial_status != CommercialStatus::PendingReview {
            return Err(Error::InvalidStateTransition {
                from: format!("{:?}", self.commercial_status),
                to: "review_transition".to_string(),
            });
        }
        ensure_transition(self.review_status, to)?;
        self.review_status = to;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 绑定当前生效版本（审批通过或同步应用后切换版本指针）。
    ///
    /// # 参数
    /// * `revision_id` - 新销售版本主键
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 无返回值；更新当前版本指针并记录更新人。
    pub fn attach_revision(&mut self, revision_id: impl Into<String>, updated_by: impl Into<String>) {
        self.stable.current_revision_id = Some(revision_id.into());
        self.stable.touch(updated_by);
    }

    /// 校验是否允许直接编辑商业字段。
    ///
    /// # 返回
    /// 可编辑时返回 `Ok(())`。
    ///
    /// # 错误
    /// `EFFECTIVE`/`VOIDED` 时返回错误（§7.1：生效后不直接编辑）。
    fn ensure_editable(&self) -> Result<()> {
        match self.commercial_status {
            CommercialStatus::Draft | CommercialStatus::PendingReview => Ok(()),
            CommercialStatus::Effective | CommercialStatus::Voided => {
                Err(Error::from("已生效或已作废的销售单不允许直接编辑"))
            }
        }
    }
}
