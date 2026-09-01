//! `business_document`：跨域单据稳定注册表（数据模型 §6.1）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::BusinessDocumentId;
use crate::validation::{normalize_optional_text, normalize_required_text};

use bpm::ApprovalProcessDefinitionId;

/// 单据编号最大长度。
const DOCUMENT_NO_MAX_LEN: usize = 128;

/// 强类型业务表类型（数据模型 §6.1 `business_document.document_type`）。
///
/// 只收录一期（§5.3）形成正式事实、需要全局编号搜索与跨域关联的强类型单据表；
/// 采购二次确认等「行为不产生单据」的对象不在此列。二期新增单据类型属于
/// 地基修订候选（需更新本枚举与注册表校验）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DocumentType {
    /// 销售单。
    SalesOrder,
    /// 卡券销售单。
    VoucherSalesOrder,
    /// 销售变更单。
    SalesChangeOrder,
    /// 采购单。
    PurchaseOrder,
    /// 采购变更单。
    PurchaseChangeOrder,
    /// 采购收货单。
    PurchaseReceipt,
    /// 仓发单。
    Delivery,
    /// 电子交付单。
    ElectronicDelivery,
    /// 服务履约单。
    ServiceFulfillment,
    /// 客户验收单。
    CustomerAcceptance,
    /// 库存调整单。
    StockAdjustment,
    /// 客户回款单。
    CustomerReceipt,
    /// 供应商付款单。
    SupplierPayment,
    /// 发票。
    Invoice,
    /// 销售退货单。
    SalesReturnCase,
    /// 采购退货单。
    PurchaseReturnOrder,
    /// 客户退款单。
    CustomerRefund,
    /// 供应商退款单。
    SupplierRefund,
    /// 回款冲正单。
    ReceiptReversal,
    /// 付款冲正单。
    PaymentReversal,
}

impl DocumentType {
    /// 返回全部已登记单据类型的权威穷尽集合。
    ///
    /// # 返回
    /// 按审批政策矩阵的稳定顺序提供全部二十个单据类型。
    ///
    /// # 约束
    /// 解析、审批政策目录和穷尽测试必须复用本集合，不维护第二份变体清单。
    pub const ALL: [Self; 20] = [
        Self::SalesOrder,
        Self::VoucherSalesOrder,
        Self::SalesChangeOrder,
        Self::PurchaseOrder,
        Self::PurchaseChangeOrder,
        Self::StockAdjustment,
        Self::CustomerReceipt,
        Self::SupplierPayment,
        Self::CustomerRefund,
        Self::SupplierRefund,
        Self::ReceiptReversal,
        Self::PaymentReversal,
        Self::PurchaseReceipt,
        Self::Delivery,
        Self::ElectronicDelivery,
        Self::ServiceFulfillment,
        Self::CustomerAcceptance,
        Self::Invoice,
        Self::SalesReturnCase,
        Self::PurchaseReturnOrder,
    ];

    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::SalesOrder => "销售单",
            Self::VoucherSalesOrder => "卡券销售单",
            Self::SalesChangeOrder => "销售变更单",
            Self::PurchaseOrder => "采购单",
            Self::PurchaseChangeOrder => "采购变更单",
            Self::PurchaseReceipt => "采购收货单",
            Self::Delivery => "仓发单",
            Self::ElectronicDelivery => "电子交付单",
            Self::ServiceFulfillment => "服务履约单",
            Self::CustomerAcceptance => "客户验收单",
            Self::StockAdjustment => "库存调整单",
            Self::CustomerReceipt => "客户回款单",
            Self::SupplierPayment => "供应商付款单",
            Self::Invoice => "发票",
            Self::SalesReturnCase => "销售退货单",
            Self::PurchaseReturnOrder => "采购退货单",
            Self::CustomerRefund => "客户退款单",
            Self::SupplierRefund => "供应商退款单",
            Self::ReceiptReversal => "回款冲正单",
            Self::PaymentReversal => "付款冲正单",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SalesOrder => "sales_order",
            Self::VoucherSalesOrder => "voucher_sales_order",
            Self::SalesChangeOrder => "sales_change_order",
            Self::PurchaseOrder => "purchase_order",
            Self::PurchaseChangeOrder => "purchase_change_order",
            Self::PurchaseReceipt => "purchase_receipt",
            Self::Delivery => "delivery",
            Self::ElectronicDelivery => "electronic_delivery",
            Self::ServiceFulfillment => "service_fulfillment",
            Self::CustomerAcceptance => "customer_acceptance",
            Self::StockAdjustment => "stock_adjustment",
            Self::CustomerReceipt => "customer_receipt",
            Self::SupplierPayment => "supplier_payment",
            Self::Invoice => "invoice",
            Self::SalesReturnCase => "sales_return_case",
            Self::PurchaseReturnOrder => "purchase_return_order",
            Self::CustomerRefund => "customer_refund",
            Self::SupplierRefund => "supplier_refund",
            Self::ReceiptReversal => "receipt_reversal",
            Self::PaymentReversal => "payment_reversal",
        }
    }

    /// 仅接受已登记的精确稳定代码。
    ///
    /// # 参数
    /// * `code` - 待解析的单据类型稳定代码
    ///
    /// # 返回
    /// 代码与冻结集合中的某一项完全一致时返回对应类型。
    ///
    /// # 错误
    /// 空值、未知代码、大小写变化或任何前后空白均返回错误。
    ///
    /// # 关键业务约束
    /// 本方法不裁剪、不折叠大小写、不接受别名，也不提供默认类型。
    pub fn try_from_code(code: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|document_type| document_type.as_str() == code)
            .ok_or_else(|| Error::from(format!("未登记单据类型: {code}")))
    }
}

/// 单据审批定义绑定。ID、版本和时间必须整体设置或整体为空。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalDefinitionBinding {
    /// 绑定的审批定义。
    pub approval_process_definition_id: ApprovalProcessDefinitionId,
    /// 绑定的定义业务版本。
    pub approval_definition_version: u32,
    /// 独立绑定 CAS 版本，初次为 1。
    pub approval_binding_version: u64,
    /// 绑定时间。
    pub approval_definition_bound_at: Instant,
}

impl ApprovalDefinitionBinding {
    /// 创建完整绑定。初次绑定版本为 1。
    ///
    /// # 参数
    /// * `approval_process_definition_id` - 定义主键
    /// * `approval_definition_version` - 定义业务版本
    /// * `approval_definition_bound_at` - 绑定时间
    ///
    /// # 错误
    /// 定义版本为零时返回错误。
    pub fn new(
        approval_process_definition_id: ApprovalProcessDefinitionId,
        approval_definition_version: u32,
        approval_definition_bound_at: Instant,
    ) -> Result<Self> {
        if approval_definition_version == 0 {
            return Err(Error::from("审批定义版本必须从 1 开始"));
        }
        Ok(Self {
            approval_process_definition_id,
            approval_definition_version,
            approval_binding_version: 1,
            approval_definition_bound_at,
        })
    }

    /// 整体替换为更高绑定版本。
    ///
    /// # 参数
    /// * `approval_process_definition_id` - 新定义
    /// * `approval_definition_version` - 新定义版本
    /// * `expected_binding_version` - 期望的当前绑定版本
    /// * `at` - 升级时间
    ///
    /// # 错误
    /// 版本不匹配或新定义版本为零时返回错误。
    pub fn upgrade(
        &self,
        approval_process_definition_id: ApprovalProcessDefinitionId,
        approval_definition_version: u32,
        expected_binding_version: u64,
        at: Instant,
    ) -> Result<Self> {
        if self.approval_binding_version != expected_binding_version {
            return Err(Error::from("审批绑定期望版本不匹配"));
        }
        if approval_definition_version == 0 {
            return Err(Error::from("审批定义版本必须从 1 开始"));
        }
        Ok(Self {
            approval_process_definition_id,
            approval_definition_version,
            approval_binding_version: self
                .approval_binding_version
                .checked_add(1)
                .ok_or_else(|| Error::from("审批绑定版本溢出"))?,
            approval_definition_bound_at: at,
        })
    }
}

/// 未提交单据审批绑定升级失败原因。
#[derive(Debug, thiserror::Error)]
pub enum ApprovalBindingUpgradeError {
    /// 单据尚未持有审批定义绑定。
    #[error("尚未绑定审批定义")]
    MissingBinding,
    /// 单据已经形成正式事实。
    #[error("已提交单据不能升级审批绑定")]
    Formalized,
    /// 单据已经启动过审批实例。
    #[error("已启动单据不能升级审批绑定")]
    ApprovalStarted,
    /// 单据版本或绑定版本与调用方预期不一致。
    #[error("数据已被其他请求修改，请刷新后重试")]
    VersionConflict,
    /// 升级原因没有有效内容。
    #[error("升级原因不能为空")]
    EmptyReason,
    /// 绑定值对象拒绝新版本。
    #[error(transparent)]
    BindingInvariant(#[from] Error),
}

/// 未提交单据审批绑定升级输入。
#[derive(Debug)]
pub struct ApprovalBindingUpgradeInput<'a> {
    /// 当前发布审批定义 ID。
    pub approval_process_definition_id: ApprovalProcessDefinitionId,
    /// 当前发布审批定义业务版本。
    pub approval_definition_version: u32,
    /// 调用方期望的审批绑定版本。
    pub expected_binding_version: u64,
    /// 本次升级原因。
    pub reason: &'a str,
    /// 绑定升级时间。
    pub at: Instant,
}

/// 单据注册创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BusinessDocumentData {
    /// 强类型业务表类型。
    pub document_type: DocumentType,
    /// 全局可查询业务编号；草稿尚未分配时为空。
    pub document_no: String,
}

/// 跨域单据稳定注册表实体（数据模型 §6.1）。
///
/// 注册表只保存类型和编号，不承载业务字段（§5.1）；`(document_type,
/// document_no)` 唯一约束与 `document_no` 全局搜索索引由 P2 建立，
/// 与强类型业务表的一对一注册校验由 P3 事务完成（§6.1）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct BusinessDocument {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 强类型业务表类型。
    pub document_type: DocumentType,
    /// 全局可查询业务编号；尚未分配时为空字符串。
    pub document_no: String,
    /// 正式编号分配时间；与非空 `document_no` 成对出现。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_no_assigned_at: Option<Instant>,
    /// 首次成为正式事实的时间（创建时为空，由强类型单据正式化时写入）。
    pub formalized_at: Option<Instant>,
    /// 审批定义绑定；无需审批或尚未绑定时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_binding: Option<ApprovalDefinitionBinding>,
    /// 首次启动审批的时间；一经写入永久证明该单据已经启动过审批。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_started_at: Option<Instant>,
}

impl BusinessDocument {
    /// 创建单据注册。
    ///
    /// 完成 document_no 的校验与规范化（去首尾空白、长度上限）；
    /// 尚未分配正式号的草稿允许以空编号注册。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::BusinessDocumentId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的注册实体（`formalized_at` 为 `None`）。
    ///
    /// # 错误
    /// 当 document_no 超长时返回错误。
    pub fn new(id: BusinessDocumentId, data: BusinessDocumentData) -> Result<Self> {
        let document_no = normalize_optional_text(Some(data.document_no), "单据编号", DOCUMENT_NO_MAX_LEN)?
            .unwrap_or_default();
        let base = BaseModel::new(id.to_string());
        let document_no_assigned_at = if document_no.is_empty() {
            None
        } else {
            Some(Instant::from_unix_secs(
                i64::try_from(base.created_at).map_err(|_| Error::from("创建时间无法表示为编号分配时间"))?,
            ))
        };
        Ok(Self {
            base,
            document_type: data.document_type,
            document_no,
            document_no_assigned_at,
            formalized_at: None,
            approval_binding: None,
            approval_started_at: None,
        })
    }

    /// 一次性分配正式单据编号。
    ///
    /// 成功后编号永久不可修改或清空。
    ///
    /// # 参数
    /// * `document_no` - 正式编号
    /// * `at` - 分配时间
    ///
    /// # 错误
    /// 编号为空/超长，或已经分配过时返回错误。
    pub fn assign_document_no(&mut self, document_no: impl Into<String>, at: Instant) -> Result<()> {
        if !self.document_no.is_empty() || self.document_no_assigned_at.is_some() {
            return Err(Error::from("单据编号只能分配一次"));
        }
        let document_no = normalize_required_text(
            document_no.into(),
            "单据编号不能为空",
            DOCUMENT_NO_MAX_LEN,
            "单据编号过长",
        )?;
        self.document_no = document_no;
        self.document_no_assigned_at = Some(at);
        Ok(())
    }

    /// 为尚未绑定的单据设置完整审批绑定。
    ///
    /// # 参数
    /// * `binding` - 整体绑定值对象
    ///
    /// # 错误
    /// 已经存在绑定时返回错误。
    pub fn bind_approval_definition(&mut self, binding: ApprovalDefinitionBinding) -> Result<()> {
        if self.approval_binding.is_some() {
            return Err(Error::from("审批绑定已存在"));
        }
        self.approval_binding = Some(binding);
        Ok(())
    }

    /// 校验 `NO_APPROVAL` 单据注册不得写入任何审批绑定。
    ///
    /// # 参数
    /// * `expected_document_type` - 强类型业务实体声明的单据类型
    /// * `returned_binding` - 统一绑定端口返回的可选绑定
    ///
    /// # 返回
    /// 注册类型一致、注册行未预置绑定且端口返回空绑定时返回 `Ok(())`。
    ///
    /// # 错误
    /// 类型不一致、注册行已有绑定或端口返回绑定时返回错误。
    pub fn ensure_no_approval_registration(
        &self,
        expected_document_type: DocumentType,
        returned_binding: Option<&ApprovalDefinitionBinding>,
    ) -> Result<()> {
        if self.document_type != expected_document_type {
            return Err(Error::from("无审批单据注册类型与业务实体不一致"));
        }
        if self.approval_binding.is_some() {
            return Err(Error::from("无审批单据注册行不得预置审批绑定"));
        }
        if returned_binding.is_some() {
            return Err(Error::from("无审批单据不得写入审批绑定"));
        }
        Ok(())
    }

    /// 整体升级未提交单据的审批绑定。
    ///
    /// # 参数
    /// * `approval_process_definition_id` - 新定义
    /// * `approval_definition_version` - 新定义版本
    /// * `expected_binding_version` - 期望的当前绑定版本
    /// * `at` - 升级时间
    ///
    /// # 错误
    /// 尚无绑定或版本不匹配时返回错误。
    pub fn upgrade_approval_binding(
        &mut self,
        approval_process_definition_id: ApprovalProcessDefinitionId,
        approval_definition_version: u32,
        expected_binding_version: u64,
        at: Instant,
    ) -> Result<()> {
        let Some(current) = self.approval_binding.as_ref() else {
            return Err(Error::from("尚未绑定审批定义"));
        };
        self.approval_binding = Some(current.upgrade(
            approval_process_definition_id,
            approval_definition_version,
            expected_binding_version,
            at,
        )?);
        Ok(())
    }

    /// 校验未提交单据是否允许升级审批绑定。
    ///
    /// # 参数
    /// * `expected_binding_version` - 调用方期望的审批绑定版本
    /// * `reason` - 本次升级原因
    ///
    /// # 返回
    /// 全部未提交升级约束满足时返回 `Ok(())`。
    ///
    /// # 错误
    /// 缺少绑定、已提交、已启动、双版本冲突或原因为空时返回对应错误。
    ///
    /// # 关键业务约束
    /// 审批启动事实必须持久化在本注册聚合中，以便启动与升级竞争同一行写冲突；
    /// 不得以查询 BPM 实例替代该并发守卫。
    pub fn ensure_unsubmitted_approval_binding_upgrade(
        &self,
        expected_binding_version: u64,
        reason: &str,
    ) -> std::result::Result<(), ApprovalBindingUpgradeError> {
        let Some(binding) = self.approval_binding.as_ref() else {
            return Err(ApprovalBindingUpgradeError::MissingBinding);
        };
        if self.formalized_at.is_some() {
            return Err(ApprovalBindingUpgradeError::Formalized);
        }
        if self.approval_started_at.is_some() {
            return Err(ApprovalBindingUpgradeError::ApprovalStarted);
        }
        if binding.approval_binding_version != expected_binding_version {
            return Err(ApprovalBindingUpgradeError::VersionConflict);
        }
        if reason.trim().is_empty() {
            return Err(ApprovalBindingUpgradeError::EmptyReason);
        }
        Ok(())
    }

    /// 整体升级满足未提交约束的审批绑定。
    ///
    /// # 参数
    /// * `input` - 当前发布定义、审批启动事实、期望版本、升级原因与发生时间
    ///
    /// # 返回
    /// 成功时整体替换审批绑定并将绑定 CAS 版本加一。
    ///
    /// # 错误
    /// 未提交升级约束失败或绑定值对象拒绝新版本时返回错误。
    ///
    /// # 关键业务约束
    /// 校验与写入使用同一实体快照，禁止只更新定义 ID、版本或时间中的部分字段。
    pub fn upgrade_unsubmitted_approval_binding(
        &mut self,
        input: ApprovalBindingUpgradeInput<'_>,
    ) -> std::result::Result<(), ApprovalBindingUpgradeError> {
        self.ensure_unsubmitted_approval_binding_upgrade(input.expected_binding_version, input.reason)?;
        let current = self
            .approval_binding
            .as_ref()
            .ok_or(ApprovalBindingUpgradeError::MissingBinding)?;
        self.approval_binding = Some(current.upgrade(
            input.approval_process_definition_id,
            input.approval_definition_version,
            input.expected_binding_version,
            input.at,
        )?);
        Ok(())
    }

    /// 永久标记该注册单据已经启动审批。
    ///
    /// # 参数
    /// * `at` - 首次启动发生时间
    ///
    /// # 返回
    /// 已经持有完整审批绑定时写入首次启动时间；重复调用保持首次时间不变。
    ///
    /// # 错误
    /// 缺少审批绑定时返回错误。
    ///
    /// # 关键业务约束
    /// 生产启动路径必须在创建 BPM 实例的同一事务内持久化本事实。字段只允许
    /// 从空变为有值，不得由取消、驳回、完成、再次提交或数据修复覆盖、清空。
    pub fn mark_approval_started(&mut self, at: Instant) -> Result<()> {
        if self.approval_binding.is_none() {
            return Err(Error::from("未绑定审批定义的单据不能启动审批"));
        }
        if self.approval_started_at.is_none() {
            self.approval_started_at = Some(at);
        }
        Ok(())
    }

    /// 标记单据首次正式化。
    ///
    /// `formalized_at` 只记录首次成为正式事实的时间，重复调用不覆盖。
    ///
    /// # 参数
    /// * `at` - 正式化时刻
    ///
    /// # 返回
    /// 无返回值；首次调用写入 `formalized_at`，后续调用保持原值。
    pub fn formalize(&mut self, at: Instant) {
        if self.formalized_at.is_none() {
            self.formalized_at = Some(at);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ApprovalBindingUpgradeError, ApprovalBindingUpgradeInput, ApprovalDefinitionBinding,
        BusinessDocument, BusinessDocumentData, DocumentType,
    };
    use crate::common::time::Instant;
    use crate::ids::BusinessDocumentId;
    use bpm::ApprovalProcessDefinitionId;
    use serde_json;

    fn data() -> BusinessDocumentData {
        BusinessDocumentData {
            document_type: DocumentType::SalesOrder,
            document_no: " SO-2025-001 ".to_string(),
        }
    }

    /// happy path：编号去首尾空白，类型与编号正确落库，初始未正式化。
    #[test]
    fn new_trims_document_no_and_starts_unformalized() {
        let doc = BusinessDocument::new(BusinessDocumentId::new("bd-1"), data()).unwrap();
        assert_eq!(doc.document_no, "SO-2025-001");
        assert_eq!(doc.document_type, DocumentType::SalesOrder);
        assert!(doc.formalized_at.is_none());
    }

    /// 草稿允许空编号，正式号只能分配一次。
    #[test]
    fn assign_document_no_is_one_shot() {
        let payload = BusinessDocumentData {
            document_no: "   ".to_string(),
            ..data()
        };
        let mut doc = BusinessDocument::new(BusinessDocumentId::new("bd-1"), payload).unwrap();
        assert!(doc.document_no.is_empty());
        assert!(doc.document_no_assigned_at.is_none());
        let at = Instant::from_unix_secs(1_700_000_000);
        doc.assign_document_no(" SO-1 ", at).unwrap();
        assert_eq!(doc.document_no, "SO-1");
        assert_eq!(doc.document_no_assigned_at, Some(at));
        assert!(doc
            .assign_document_no("SO-2", Instant::from_unix_secs(2))
            .is_err());
        assert!(doc.assign_document_no("  ", Instant::from_unix_secs(2)).is_err());
    }

    /// 绑定必须整体设置；升级要求期望版本。
    #[test]
    fn approval_binding_is_atomic() {
        let mut doc = BusinessDocument::new(BusinessDocumentId::new("bd-1"), data()).unwrap();
        assert!(doc.approval_binding.is_none());
        let binding = ApprovalDefinitionBinding::new(
            ApprovalProcessDefinitionId::new("def-1"),
            1,
            Instant::from_unix_secs(10),
        )
        .unwrap();
        doc.bind_approval_definition(binding).unwrap();
        assert_eq!(doc.approval_binding.as_ref().unwrap().approval_binding_version, 1);
        assert!(doc
            .bind_approval_definition(
                ApprovalDefinitionBinding::new(
                    ApprovalProcessDefinitionId::new("def-2"),
                    1,
                    Instant::from_unix_secs(11),
                )
                .unwrap()
            )
            .is_err());
        doc.upgrade_approval_binding(
            ApprovalProcessDefinitionId::new("def-2"),
            2,
            1,
            Instant::from_unix_secs(12),
        )
        .unwrap();
        assert_eq!(doc.approval_binding.as_ref().unwrap().approval_binding_version, 2);
        assert!(doc
            .upgrade_approval_binding(
                ApprovalProcessDefinitionId::new("def-3"),
                3,
                1,
                Instant::from_unix_secs(13),
            )
            .is_err());
        assert!(ApprovalDefinitionBinding::new(
            ApprovalProcessDefinitionId::new("def-0"),
            0,
            Instant::from_unix_secs(1),
        )
        .is_err());
    }

    /// 无审批注册要求类型一致且注册行与绑定端口都保持空绑定。
    #[test]
    fn no_approval_registration_rejects_any_binding_or_type_mismatch() {
        let document = BusinessDocument::new(
            BusinessDocumentId::new("delivery-1"),
            BusinessDocumentData {
                document_type: DocumentType::ElectronicDelivery,
                document_no: "ED-1".to_string(),
            },
        )
        .unwrap();
        assert!(document
            .ensure_no_approval_registration(DocumentType::ElectronicDelivery, None)
            .is_ok());
        assert!(document
            .ensure_no_approval_registration(DocumentType::Delivery, None)
            .is_err());

        let binding = ApprovalDefinitionBinding::new(
            ApprovalProcessDefinitionId::new("def-1"),
            1,
            Instant::from_unix_secs(10),
        )
        .unwrap();
        assert!(document
            .ensure_no_approval_registration(DocumentType::ElectronicDelivery, Some(&binding),)
            .is_err());
        let mut prebound = document.clone();
        prebound.bind_approval_definition(binding).unwrap();
        assert!(prebound
            .ensure_no_approval_registration(DocumentType::ElectronicDelivery, None)
            .is_err());
    }

    /// 未提交绑定升级由实体统一校验绑定 CAS、提交状态、启动事实与原因。
    #[test]
    fn unsubmitted_binding_upgrade_enforces_all_document_rules() {
        let mut document = BusinessDocument::new(BusinessDocumentId::new("bd-1"), data()).unwrap();
        document
            .bind_approval_definition(
                ApprovalDefinitionBinding::new(
                    ApprovalProcessDefinitionId::new("def-1"),
                    1,
                    Instant::from_unix_secs(10),
                )
                .unwrap(),
            )
            .unwrap();
        document
            .upgrade_unsubmitted_approval_binding(ApprovalBindingUpgradeInput {
                approval_process_definition_id: ApprovalProcessDefinitionId::new("def-2"),
                approval_definition_version: 2,
                expected_binding_version: 1,
                reason: "切换到当前发布版本",
                at: Instant::from_unix_secs(11),
            })
            .unwrap();
        assert_eq!(
            document
                .approval_binding
                .as_ref()
                .unwrap()
                .approval_binding_version,
            2
        );

        let mut missing = BusinessDocument::new(BusinessDocumentId::new("bd-2"), data()).unwrap();
        assert!(matches!(
            missing.upgrade_unsubmitted_approval_binding(ApprovalBindingUpgradeInput {
                approval_process_definition_id: ApprovalProcessDefinitionId::new("def-2"),
                approval_definition_version: 2,
                expected_binding_version: 1,
                reason: "原因",
                at: Instant::from_unix_secs(11),
            }),
            Err(ApprovalBindingUpgradeError::MissingBinding)
        ));

        let mut formalized = document.clone();
        formalized.formalize(Instant::from_unix_secs(12));
        assert!(matches!(
            formalized.ensure_unsubmitted_approval_binding_upgrade(2, "原因"),
            Err(ApprovalBindingUpgradeError::Formalized)
        ));
        let mut started = document.clone();
        started
            .mark_approval_started(Instant::from_unix_secs(12))
            .unwrap();
        assert!(matches!(
            started.ensure_unsubmitted_approval_binding_upgrade(2, "原因"),
            Err(ApprovalBindingUpgradeError::ApprovalStarted)
        ));
        assert!(matches!(
            document.ensure_unsubmitted_approval_binding_upgrade(1, "原因"),
            Err(ApprovalBindingUpgradeError::VersionConflict)
        ));
        assert!(matches!(
            document.ensure_unsubmitted_approval_binding_upgrade(2, "   "),
            Err(ApprovalBindingUpgradeError::EmptyReason)
        ));
        started
            .mark_approval_started(Instant::from_unix_secs(13))
            .unwrap();
        assert_eq!(started.approval_started_at, Some(Instant::from_unix_secs(12)));
    }

    /// 失败路径：超长编号被拒。
    #[test]
    fn new_rejects_overlong_document_no() {
        let payload = BusinessDocumentData {
            document_no: "x".repeat(129),
            ..data()
        };
        assert!(BusinessDocument::new(BusinessDocumentId::new("bd-1"), payload).is_err());
    }

    /// 正式化时间只记录首次。
    #[test]
    fn formalize_only_records_first_time() {
        let mut doc = BusinessDocument::new(BusinessDocumentId::new("bd-1"), data()).unwrap();
        let first = Instant::from_unix_secs(1_700_000_000);
        let second = Instant::from_unix_secs(1_700_086_400);
        doc.formalize(first);
        doc.formalize(second);
        assert_eq!(doc.formalized_at.unwrap(), first);
    }

    /// 验证全部二十个冻结代码都能精确解析。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 所有代码映射到对应枚举时测试通过。
    ///
    /// # 错误
    /// 任一代码被拒绝或映射到错误类型时测试失败。
    ///
    /// # 关键业务约束
    /// 用例覆盖 `DocumentType` 的完整稳定代码集合。
    #[test]
    fn document_type_try_from_code_accepts_all_stable_codes() {
        assert_eq!(DocumentType::ALL.len(), 20);
        for expected in DocumentType::ALL {
            assert_eq!(DocumentType::try_from_code(expected.as_str()).unwrap(), expected);
        }
    }

    /// 验证非精确代码按失败关闭处理。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 所有非法输入均被拒绝时测试通过。
    ///
    /// # 错误
    /// 空值、未知值、大小写变化或空白变体被接受时测试失败。
    ///
    /// # 关键业务约束
    /// 解析不得裁剪、折叠大小写、接受别名或回落默认类型。
    #[test]
    fn document_type_try_from_code_rejects_non_exact_codes() {
        for code in [
            "",
            "unknown",
            "Sales_order",
            "SALES_ORDER",
            "sales_order ",
            " sales_order",
            "sales_order\n",
        ] {
            assert!(
                DocumentType::try_from_code(code).is_err(),
                "unexpected code: {code:?}"
            );
        }
    }

    /// 合同 §4.3 的 20 个固定类型：as_str、label 与 serde 必须穷尽一致。
    #[test]
    fn document_type_codes_and_labels_are_stable() {
        const ROWS: &[(DocumentType, &str, &str)] = &[
            (DocumentType::SalesOrder, "sales_order", "销售单"),
            (
                DocumentType::VoucherSalesOrder,
                "voucher_sales_order",
                "卡券销售单",
            ),
            (DocumentType::SalesChangeOrder, "sales_change_order", "销售变更单"),
            (DocumentType::PurchaseOrder, "purchase_order", "采购单"),
            (
                DocumentType::PurchaseChangeOrder,
                "purchase_change_order",
                "采购变更单",
            ),
            (DocumentType::PurchaseReceipt, "purchase_receipt", "采购收货单"),
            (DocumentType::Delivery, "delivery", "仓发单"),
            (
                DocumentType::ElectronicDelivery,
                "electronic_delivery",
                "电子交付单",
            ),
            (
                DocumentType::ServiceFulfillment,
                "service_fulfillment",
                "服务履约单",
            ),
            (
                DocumentType::CustomerAcceptance,
                "customer_acceptance",
                "客户验收单",
            ),
            (DocumentType::StockAdjustment, "stock_adjustment", "库存调整单"),
            (DocumentType::CustomerReceipt, "customer_receipt", "客户回款单"),
            (DocumentType::SupplierPayment, "supplier_payment", "供应商付款单"),
            (DocumentType::Invoice, "invoice", "发票"),
            (DocumentType::SalesReturnCase, "sales_return_case", "销售退货单"),
            (
                DocumentType::PurchaseReturnOrder,
                "purchase_return_order",
                "采购退货单",
            ),
            (DocumentType::CustomerRefund, "customer_refund", "客户退款单"),
            (DocumentType::SupplierRefund, "supplier_refund", "供应商退款单"),
            (DocumentType::ReceiptReversal, "receipt_reversal", "回款冲正单"),
            (DocumentType::PaymentReversal, "payment_reversal", "付款冲正单"),
        ];
        assert_eq!(ROWS.len(), 20);

        for (variant, code, label) in ROWS {
            assert_eq!(variant.as_str(), *code);
            assert_eq!(variant.label(), *label);
            let json = serde_json::to_string(variant).unwrap();
            assert_eq!(json, format!("\"{code}\""));
            let back: DocumentType = serde_json::from_str(&json).unwrap();
            assert_eq!(back, *variant);
        }

        assert_eq!(
            serde_json::to_string(&DocumentType::VoucherSalesOrder).unwrap(),
            "\"voucher_sales_order\""
        );
        let voucher: DocumentType = serde_json::from_str("\"voucher_sales_order\"").unwrap();
        assert_eq!(voucher, DocumentType::VoucherSalesOrder);
    }

    /// BSON 往返（实体层持久化形态与 P0 约定一致）。
    #[test]
    fn entity_roundtrips_through_bson() {
        let doc = BusinessDocument::new(BusinessDocumentId::new("bd-1"), data()).unwrap();
        let roundtrip: BusinessDocument =
            bson::deserialize_from_document(bson::serialize_to_document(&doc).unwrap()).unwrap();
        assert_eq!(roundtrip, doc);
    }
}
