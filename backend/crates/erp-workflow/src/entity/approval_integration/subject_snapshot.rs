//! 业务对象快照。与 BPM 实例一对一，使用有界强类型结构。

use bpm::ApprovalProcessInstanceId;
use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::ids::{ApprovalSubjectSnapshotId, CustomerAccountId, SupplierAccountId, WarehouseId};
use erp_core::money::{Amount, Quantity};
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::ApprovalMaterialFile;
use crate::entity::document_registry::DocumentType;

const DOCUMENT_NO_MAX_LEN: usize = 128;
const ORG_ID_MAX_LEN: usize = 128;
const ACTOR_ID_MAX_LEN: usize = 128;
const OBJECT_ID_MAX_LEN: usize = 128;
const MATERIAL_FILE_LIMIT: usize = 100;

/// 对手方引用。按类型穷尽枚举，可为空。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ApprovalSubjectCounterparty {
    /// 客户。
    Customer {
        /// 客户账号。
        customer_id: CustomerAccountId,
    },
    /// 供应商。
    Supplier {
        /// 供应商账号。
        supplier_id: SupplierAccountId,
    },
    /// 仓库。
    Warehouse {
        /// 仓库。
        warehouse_id: WarehouseId,
    },
}

/// 政策签署的有界业务快照载荷。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalSubjectSnapshotPayload {
    /// 单据业务编号。
    pub document_no: String,
    /// 责任组织，同时是审批任务责任组织来源。
    pub responsible_org_id: String,
    /// 提交人账号。
    pub submitted_by: String,
    /// 提交时间。
    pub submitted_at: Instant,
    /// 对手方。
    pub counterparty: Option<ApprovalSubjectCounterparty>,
    /// 金额合计。
    pub total_amount: Option<Amount>,
    /// 数量合计。
    pub total_quantity: Option<Quantity>,
    /// 行数。
    pub line_count: u32,
}

/// 启动审批时冻结的业务对象快照。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ApprovalSubjectSnapshot {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 不可变关联的审批实例。
    pub approval_process_instance_id: ApprovalProcessInstanceId,
    /// 单据类型。
    pub document_type: DocumentType,
    /// 业务对象主键。
    pub business_object_id: String,
    /// 冻结提交版本。
    pub subject_version: u32,
    /// 有界业务字段。
    pub payload: ApprovalSubjectSnapshotPayload,
    /// 旧快照没有该字段；缺失时不得从当前草稿补成历史内容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<super::display_snapshot::ApprovalDisplaySnapshot>,
    /// 提交时从当前业务对象证明并冻结的材料引用；历史缺失保持空，不回填当前附件。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub material_files: Vec<ApprovalMaterialFile>,
}

impl ApprovalSubjectSnapshot {
    /// 创建与实例一一对应的业务对象快照。
    ///
    /// 金额与数量按单据类型强制校验，不得保存任意 Map 或未约束 JSON。
    ///
    /// # 参数
    /// * `id` - 快照主键
    /// * `approval_process_instance_id` - 关联实例
    /// * `document_type` - 单据类型
    /// * `business_object_id` - 业务对象主键
    /// * `subject_version` - 冻结提交版本
    /// * `payload` - 有界快照字段
    ///
    /// # 返回
    /// 返回尚未附带展示快照和材料文件的业务对象快照。
    ///
    /// # 错误
    /// 必填字段缺失、超长或类型要求的金额/数量缺失时返回错误。
    pub fn new(
        id: ApprovalSubjectSnapshotId,
        approval_process_instance_id: ApprovalProcessInstanceId,
        document_type: DocumentType,
        business_object_id: impl Into<String>,
        subject_version: u32,
        payload: ApprovalSubjectSnapshotPayload,
    ) -> Result<Self> {
        let payload = normalize_payload(document_type, payload)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            approval_process_instance_id,
            document_type,
            business_object_id: normalize_required_text(
                business_object_id.into(),
                "业务对象ID不能为空",
                OBJECT_ID_MAX_LEN,
                "业务对象ID过长",
            )?,
            subject_version,
            payload,
            display: None,
            material_files: Vec::new(),
        })
    }

    /// 为新提交快照冻结已由业务领域证明的材料文件引用。
    ///
    /// # 参数
    /// * `ids` - 当前业务版本实际关联的文件资产 ID，最多 100 个。
    /// # 返回
    /// 返回去重并稳定排序的快照；本方法不生成文件读取授权。
    /// # 错误
    /// 超量、空白或非法长度 ID 时拒绝整个快照。
    pub fn with_material_files(mut self, mut files: Vec<ApprovalMaterialFile>) -> Result<Self> {
        if files.len() > MATERIAL_FILE_LIMIT {
            return Err(Error::from("审批材料超过100个"));
        }
        for file in &files {
            file.validate()?;
        }
        files.sort_by(|left, right| left.file_asset_id.as_ref().cmp(right.file_asset_id.as_ref()));
        if files.windows(2).any(|pair| pair[0].file_asset_id == pair[1].file_asset_id && pair[0] != pair[1]) {
            return Err(Error::from("同一审批材料存在不同文件版本"));
        }
        files.dedup();
        self.material_files = files;
        Ok(self)
    }

    /// 证明材料属于精确运行实例及其冻结提交版本。
    ///
    /// # 参数
    /// * `instance_id` / `document_type` / `document_id` / `version` - 已授权运行主体。
    /// * `file_id` - 本次请求的文件资产 ID。
    /// # 返回
    /// 所有主体引用和冻结允许清单同时匹配时返回 true；调用方仍须证明参与或管理资格。
    /// # 错误
    /// 无；旧快照、缺失引用、串实例或串版本均返回 false。
    pub fn allows_runtime_material(
        &self,
        instance_id: &str,
        document_type: DocumentType,
        document_id: &str,
        version: u32,
        file_id: &str,
    ) -> bool {
        self.approval_process_instance_id.as_ref() == instance_id
            && self.ensure_matches_runtime_subject(document_type, document_id, version).is_ok()
            && self.material_files.len() <= MATERIAL_FILE_LIMIT
            && self
                .material_files
                .iter()
                .any(|file| file.file_asset_id.as_ref() == file_id && file.validate().is_ok())
    }

    /// 校验冻结快照与运行时主体的三项不可变引用完全一致。
    ///
    /// # 参数
    /// * `document_type` - 运行时解析出的单据类型
    /// * `business_object_id` - 运行时主体持有的业务对象主键
    /// * `subject_version` - 运行时实例冻结的提交版本
    ///
    /// # 返回
    /// 单据类型、业务对象主键和提交版本全部精确一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 按单据类型、业务对象主键、提交版本的固定顺序返回首个不匹配错误。
    ///
    /// # 关键业务约束
    /// 三项比较均不可省略；字符串不裁剪、不折叠大小写，也不接受任何别名。
    pub fn ensure_matches_runtime_subject(
        &self,
        document_type: DocumentType,
        business_object_id: &str,
        subject_version: u32,
    ) -> Result<()> {
        if self.document_type != document_type {
            return Err(Error::from("冻结快照单据类型不匹配"));
        }
        if self.business_object_id != business_object_id {
            return Err(Error::from("冻结快照业务对象ID不匹配"));
        }
        if self.subject_version != subject_version {
            return Err(Error::from("冻结快照提交版本不匹配"));
        }
        Ok(())
    }
}

/// 规范化快照载荷并按单据类型校验金额/数量必填范围。
///
/// # 错误
/// 文本非法或必填合计缺失时返回错误。
fn normalize_payload(
    document_type: DocumentType,
    payload: ApprovalSubjectSnapshotPayload,
) -> Result<ApprovalSubjectSnapshotPayload> {
    let document_no = normalize_required_text(
        payload.document_no,
        "单据编号不能为空",
        DOCUMENT_NO_MAX_LEN,
        "单据编号过长",
    )?;
    let responsible_org_id = normalize_required_text(
        payload.responsible_org_id,
        "责任组织不能为空",
        ORG_ID_MAX_LEN,
        "责任组织过长",
    )?;
    let submitted_by =
        normalize_required_text(payload.submitted_by, "提交人不能为空", ACTOR_ID_MAX_LEN, "提交人过长")?;
    ensure_required_totals(document_type, payload.total_amount.as_ref(), payload.total_quantity.as_ref())?;
    Ok(ApprovalSubjectSnapshotPayload {
        document_no,
        responsible_org_id,
        submitted_by,
        submitted_at: payload.submitted_at,
        counterparty: payload.counterparty,
        total_amount: payload.total_amount,
        total_quantity: payload.total_quantity,
        line_count: payload.line_count,
    })
}

/// 按合同 §4.4.5 校验金额与数量必填范围。
///
/// # 错误
/// 对应类型缺少必填合计时返回错误。
fn ensure_required_totals(
    document_type: DocumentType,
    total_amount: Option<&Amount>,
    total_quantity: Option<&Quantity>,
) -> Result<()> {
    if requires_amount(document_type) && total_amount.is_none() {
        return Err(Error::from("该单据类型必须冻结金额合计"));
    }
    if requires_quantity(document_type) && total_quantity.is_none() {
        return Err(Error::from("该单据类型必须冻结数量合计"));
    }
    Ok(())
}

/// 资金、销售、采购、变更、退款、冲正类必须冻结金额。
fn requires_amount(document_type: DocumentType) -> bool {
    matches!(
        document_type,
        DocumentType::SalesOrder
            | DocumentType::VoucherSalesOrder
            | DocumentType::SalesChangeOrder
            | DocumentType::PurchaseOrder
            | DocumentType::PurchaseChangeOrder
            | DocumentType::SalesInvoiceRequest
            | DocumentType::CustomerReceipt
            | DocumentType::SupplierPayment
            | DocumentType::CustomerRefund
            | DocumentType::SupplierRefund
            | DocumentType::ReceiptReversal
            | DocumentType::PaymentReversal
    )
}

/// 库存调整、销售、采购类必须冻结数量。
fn requires_quantity(document_type: DocumentType) -> bool {
    matches!(
        document_type,
        DocumentType::SalesOrder
            | DocumentType::VoucherSalesOrder
            | DocumentType::PurchaseOrder
            | DocumentType::StockAdjustment
    )
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use bpm::ApprovalProcessInstanceId;
    use erp_core::common::time::Instant;
    use erp_core::ids::{ApprovalSubjectSnapshotId, WarehouseId};
    use erp_core::money::Quantity;

    use super::{ApprovalSubjectCounterparty, ApprovalSubjectSnapshot, ApprovalSubjectSnapshotPayload};
    use crate::entity::document_registry::DocumentType;

    fn stock_payload() -> ApprovalSubjectSnapshotPayload {
        ApprovalSubjectSnapshotPayload {
            document_no: " ADJ-1 ".into(),
            responsible_org_id: " org-1 ".into(),
            submitted_by: " user-1 ".into(),
            submitted_at: Instant::from_unix_secs(1_700_000_000),
            counterparty: Some(ApprovalSubjectCounterparty::Warehouse {
                warehouse_id: WarehouseId::new("wh-1"),
            }),
            total_amount: None,
            total_quantity: Some(Quantity::from_str("2").unwrap()),
            line_count: 1,
        }
    }

    /// 快照规范化文本，并与实例一一对应。
    #[test]
    fn snapshot_normalizes_and_binds_instance() {
        let snapshot = ApprovalSubjectSnapshot::new(
            ApprovalSubjectSnapshotId::new("snap-1"),
            ApprovalProcessInstanceId::new("inst-1"),
            DocumentType::StockAdjustment,
            " adj-1 ",
            1,
            stock_payload(),
        )
        .unwrap();
        assert_eq!(snapshot.payload.document_no, "ADJ-1");
        assert_eq!(snapshot.payload.responsible_org_id, "org-1");
        assert_eq!(snapshot.business_object_id, "adj-1");
        assert_eq!(snapshot.subject_version, 1);
    }

    /// 库存调整缺少数量合计必须失败。
    #[test]
    fn stock_adjustment_requires_quantity() {
        let mut payload = stock_payload();
        payload.total_quantity = None;
        assert!(
            ApprovalSubjectSnapshot::new(
                ApprovalSubjectSnapshotId::new("snap-1"),
                ApprovalProcessInstanceId::new("inst-1"),
                DocumentType::StockAdjustment,
                "adj-1",
                1,
                payload,
            )
            .is_err()
        );
    }

    /// 回款单缺少金额合计必须失败。
    #[test]
    fn receipt_requires_amount() {
        assert!(
            ApprovalSubjectSnapshot::new(
                ApprovalSubjectSnapshotId::new("snap-1"),
                ApprovalProcessInstanceId::new("inst-1"),
                DocumentType::CustomerReceipt,
                "rcpt-1",
                1,
                stock_payload(),
            )
            .is_err()
        );
    }

    /// 验证运行时主体的三项精确引用可以匹配冻结快照。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 三项引用完全一致时测试通过。
    ///
    /// # 错误
    /// 任一相等值被误判为不匹配时测试失败。
    ///
    /// # 关键业务约束
    /// 正常路径必须同时比较单据类型、业务对象主键和提交版本。
    #[test]
    fn runtime_subject_exact_match_succeeds() {
        let snapshot = ApprovalSubjectSnapshot::new(
            ApprovalSubjectSnapshotId::new("snap-1"),
            ApprovalProcessInstanceId::new("inst-1"),
            DocumentType::StockAdjustment,
            "adj-1",
            1,
            stock_payload(),
        )
        .unwrap();

        assert!(snapshot.ensure_matches_runtime_subject(DocumentType::StockAdjustment, "adj-1", 1).is_ok());
    }

    /// 验证三项主体不匹配按固定顺序失败且不做规范化。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 每类不匹配都返回对应确定错误时测试通过。
    ///
    /// # 错误
    /// 任一比较被省略、顺序变化或空白输入被裁剪时测试失败。
    ///
    /// # 关键业务约束
    /// 单据类型优先于主键，主键优先于版本；零版本和空白变体仍按原值比较。
    #[test]
    fn runtime_subject_mismatches_are_deterministic_and_strict() {
        let snapshot = ApprovalSubjectSnapshot::new(
            ApprovalSubjectSnapshotId::new("snap-1"),
            ApprovalProcessInstanceId::new("inst-1"),
            DocumentType::StockAdjustment,
            "adj-1",
            1,
            stock_payload(),
        )
        .unwrap();

        assert_eq!(
            snapshot
                .ensure_matches_runtime_subject(DocumentType::SalesOrder, "other", 2)
                .unwrap_err()
                .to_string(),
            "冻结快照单据类型不匹配"
        );
        assert_eq!(
            snapshot
                .ensure_matches_runtime_subject(DocumentType::StockAdjustment, " adj-1 ", 1)
                .unwrap_err()
                .to_string(),
            "冻结快照业务对象ID不匹配"
        );
        assert_eq!(
            snapshot
                .ensure_matches_runtime_subject(DocumentType::StockAdjustment, "adj-1", 0)
                .unwrap_err()
                .to_string(),
            "冻结快照提交版本不匹配"
        );
    }

    fn material(id: &str) -> super::ApprovalMaterialFile {
        super::ApprovalMaterialFile {
            file_asset_id: erp_core::ids::FileAssetId::new(id),
            file_name: "proof.pdf".into(),
            content_type: "application/pdf".into(),
            byte_size: 12,
            asset_version: 1,
            content_hmac: "a".repeat(64),
        }
    }

    #[test]
    fn materials_are_limited_to_the_exact_frozen_instance_subject_and_version() {
        let snapshot = ApprovalSubjectSnapshot::new(
            ApprovalSubjectSnapshotId::new("snap-1"),
            ApprovalProcessInstanceId::new("inst-1"),
            DocumentType::StockAdjustment,
            "adj-1",
            1,
            stock_payload(),
        )
        .unwrap()
        .with_material_files(vec![material("proof-2"), material("proof-1"), material("proof-1")])
        .unwrap();
        assert_eq!(snapshot.material_files.len(), 2);
        assert!(snapshot.allows_runtime_material(
            "inst-1",
            DocumentType::StockAdjustment,
            "adj-1",
            1,
            "proof-1"
        ));
        for (instance, kind, document, version, file) in [
            ("inst-other", DocumentType::StockAdjustment, "adj-1", 1, "proof-1"),
            ("inst-1", DocumentType::SalesOrder, "adj-1", 1, "proof-1"),
            ("inst-1", DocumentType::StockAdjustment, "adj-other", 1, "proof-1"),
            ("inst-1", DocumentType::StockAdjustment, "adj-1", 2, "proof-1"),
            ("inst-1", DocumentType::StockAdjustment, "adj-1", 1, "new-current-attachment"),
        ] {
            assert!(!snapshot.allows_runtime_material(instance, kind, document, version, file));
        }
        let mut legacy = serde_json::to_value(&snapshot).unwrap();
        legacy.as_object_mut().unwrap().remove("material_files");
        let legacy: ApprovalSubjectSnapshot = serde_json::from_value(legacy).unwrap();
        assert!(!legacy.allows_runtime_material(
            "inst-1",
            DocumentType::StockAdjustment,
            "adj-1",
            1,
            "proof-1"
        ));
        assert!(snapshot.clone().with_material_files(vec![material(" ")]).is_err());
        let mut changed = material("proof-1");
        changed.content_hmac = "b".repeat(64);
        assert!(!material("proof-1").matches_current(&changed));
        assert!(snapshot.clone().with_material_files(vec![material("proof-1"), changed]).is_err());
        assert!(
            snapshot.with_material_files((0..101).map(|i| material(&format!("f-{i}"))).collect()).is_err()
        );
    }

    /// BSON 往返保持有界结构。
    #[test]
    fn snapshot_roundtrips_through_bson() {
        let snapshot = ApprovalSubjectSnapshot::new(
            ApprovalSubjectSnapshotId::new("snap-1"),
            ApprovalProcessInstanceId::new("inst-1"),
            DocumentType::StockAdjustment,
            "adj-1",
            1,
            stock_payload(),
        )
        .unwrap();
        let roundtrip: ApprovalSubjectSnapshot =
            serde_json::from_value(serde_json::to_value(&snapshot).unwrap()).unwrap();
        assert_eq!(roundtrip, snapshot);
    }
}
