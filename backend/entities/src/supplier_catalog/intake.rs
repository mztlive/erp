//! `supplier_catalog_intake_batch` / `supplier_catalog_intake_item`（数据模型 §6.14）。
//!
//! 保存 Excel 导入、API 同步或手工批量录入的来源批次与明细。批次必须包含
//! `source_type`、`supplier_id` 和 `source_reference`；API 批次才允许连接 ID，
//! Excel 批次保存文件资产引用，手工单条也生成可审计批次。唯一键为
//! `(source_type, supplier_id, source_reference)`；明细按「批次 + 供应商 SKU +
//! 来源版本」唯一（唯一性由索引保证，P3 校验）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::{
    FileAssetId, SupplierAccountId, SupplierApiConnectionId, SupplierCatalogIntakeBatchId,
    SupplierCatalogIntakeItemId, SupplierCatalogSkuId,
};
use crate::supplier_catalog::types::CatalogSourceType;
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 来源引用最大长度。
const SOURCE_REFERENCE_MAX_LEN: usize = 256;
/// 供应商 SKU 编码最大长度。
const SKU_CODE_MAX_LEN: usize = 128;
/// 来源版本标识最大长度。
const REVISION_TOKEN_MAX_LEN: usize = 256;
/// 错误文本最大长度。
const ERROR_TEXT_MAX_LEN: usize = 2000;

/// 入库批次状态（§6.14 处理语义：待处理、处理中、完成、失败）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum IntakeBatchStatus {
    /// 待处理。
    Pending,
    /// 处理中。
    Processing,
    /// 已完成。
    Completed,
    /// 失败。
    Failed,
}

impl IntakeBatchStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "待处理",
            Self::Processing => "处理中",
            Self::Completed => "已完成",
            Self::Failed => "失败",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Processing => "PROCESSING",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
        }
    }
}

/// 入库明细分类（§6.14：新增、变化、无变化、停止供应、异常）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum IntakeItemClassification {
    /// 新增。
    New,
    /// 变化。
    Changed,
    /// 无变化。
    Unchanged,
    /// 停止供应。
    Stopped,
    /// 异常。
    Exception,
}

impl IntakeItemClassification {
    /// 返回分类的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::New => "新增",
            Self::Changed => "变化",
            Self::Unchanged => "无变化",
            Self::Stopped => "停止供应",
            Self::Exception => "异常",
        }
    }

    /// 返回分类的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::New => "NEW",
            Self::Changed => "CHANGED",
            Self::Unchanged => "UNCHANGED",
            Self::Stopped => "STOPPED",
            Self::Exception => "EXCEPTION",
        }
    }
}

/// 入库明细处理结果（§6.14：成功、失败）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum IntakeItemResult {
    /// 成功。
    Success,
    /// 失败。
    Failed,
}

impl IntakeItemResult {
    /// 返回结果的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Success => "成功",
            Self::Failed => "失败",
        }
    }

    /// 返回结果的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "SUCCESS",
            Self::Failed => "FAILED",
        }
    }
}

/// 来源批次创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierCatalogIntakeBatchData {
    /// 来源类型。
    pub source_type: CatalogSourceType,
    /// 来源供应商。
    pub supplier_id: SupplierAccountId,
    /// 来源引用（来源版本/文件/消息引用，参与唯一键）。
    pub source_reference: String,
    /// API 连接；仅 API 批次可填写。
    pub source_connection_id: Option<SupplierApiConnectionId>,
    /// Excel 批次保存文件资产引用。
    pub file_asset_id: Option<FileAssetId>,
}

/// 来源入库批次实体（数据模型 §6.14）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierCatalogIntakeBatch {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 来源类型。
    pub source_type: CatalogSourceType,
    /// 来源供应商。
    pub supplier_id: SupplierAccountId,
    /// 来源引用。
    pub source_reference: String,
    /// API 连接。
    pub source_connection_id: Option<SupplierApiConnectionId>,
    /// Excel 批次文件资产引用。
    pub file_asset_id: Option<FileAssetId>,
    /// 批次状态。
    pub status: IntakeBatchStatus,
    /// 批次级错误说明（`Failed` 时必填）。
    pub error_text: Option<String>,
}

impl SupplierCatalogIntakeBatch {
    /// 创建来源入库批次。
    ///
    /// 完成来源引用校验与规范化，并强制「非 API 批次不得携带连接」不变式
    /// （§6.14：API 批次才允许连接 ID）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierCatalogIntakeBatchId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的批次实体（初始状态 `Pending`）。
    ///
    /// # 错误
    /// 来源引用为空/超长，或非 API 批次携带连接时返回错误。
    pub fn new(id: SupplierCatalogIntakeBatchId, data: SupplierCatalogIntakeBatchData) -> Result<Self> {
        let source_reference = normalize_required_text(
            data.source_reference,
            "来源引用不能为空",
            SOURCE_REFERENCE_MAX_LEN,
            "来源引用过长",
        )?;
        if data.source_type != CatalogSourceType::Api && data.source_connection_id.is_some() {
            return Err(Error::from("只有 API 批次可以填写连接"));
        }
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            source_type: data.source_type,
            supplier_id: data.supplier_id,
            source_reference,
            source_connection_id: data.source_connection_id,
            file_asset_id: data.file_asset_id,
            status: IntakeBatchStatus::Pending,
            error_text: None,
        })
    }

    /// 更新批次处理状态。
    ///
    /// 进入 `Failed` 时错误说明必填；完成/处理中状态不得携带错误说明。
    ///
    /// # 参数
    /// * `status` - 新状态
    /// * `error_text` - 批次级错误说明（可空）
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// `Failed` 缺少错误说明，或非失败状态携带错误说明时返回错误。
    pub fn update(&mut self, status: IntakeBatchStatus, error_text: Option<String>) -> Result<()> {
        let error_text = normalize_optional_text(error_text, "批次错误说明", ERROR_TEXT_MAX_LEN)?;
        if status == IntakeBatchStatus::Failed && error_text.is_none() {
            return Err(Error::from("失败批次必须填写错误说明"));
        }
        if status != IntakeBatchStatus::Failed && error_text.is_some() {
            return Err(Error::from("非失败批次不得填写错误说明"));
        }
        self.status = status;
        self.error_text = error_text;
        Ok(())
    }
}

/// 来源入库明细创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierCatalogIntakeItemData {
    /// 所属批次。
    pub supplier_catalog_intake_batch_id: SupplierCatalogIntakeBatchId,
    /// 来源行号（从 1 递增）。
    pub row_no: u32,
    /// 供应商 SKU 编码（与来源版本组成明细唯一键）。
    pub supplier_sku_code: String,
    /// 来源版本标识。
    pub source_revision_token: Option<String>,
    /// 处理分类。
    pub classification: IntakeItemClassification,
    /// 处理结果。
    pub result: IntakeItemResult,
    /// 错误说明（`Failed` 时必填）。
    pub error_text: Option<String>,
    /// 本次处理到的供应商 SKU（失败时为空）。
    pub supplier_catalog_sku_id: Option<SupplierCatalogSkuId>,
}

/// 来源入库明细实体（数据模型 §6.14）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierCatalogIntakeItem {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属批次。
    pub supplier_catalog_intake_batch_id: SupplierCatalogIntakeBatchId,
    /// 来源行号。
    pub row_no: u32,
    /// 供应商 SKU 编码。
    pub supplier_sku_code: String,
    /// 来源版本标识。
    pub source_revision_token: Option<String>,
    /// 处理分类。
    pub classification: IntakeItemClassification,
    /// 处理结果。
    pub result: IntakeItemResult,
    /// 错误说明。
    pub error_text: Option<String>,
    /// 本次处理到的供应商 SKU。
    pub supplier_catalog_sku_id: Option<SupplierCatalogSkuId>,
}

impl SupplierCatalogIntakeItem {
    /// 创建来源入库明细。
    ///
    /// 完成 SKU 编码/版本标识/错误说明的校验与规范化，并强制两条不变式：
    /// 行号从 1 开始；`Failed` 时错误说明必填且不得携带处理到的 SKU。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierCatalogIntakeItemId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的明细实体。
    ///
    /// # 错误
    /// 行号为零、SKU 编码为空/超长，或失败明细缺少错误说明时返回错误。
    pub fn new(id: SupplierCatalogIntakeItemId, data: SupplierCatalogIntakeItemData) -> Result<Self> {
        ensure_row_no(data.row_no)?;
        let supplier_sku_code = normalize_required_text(
            data.supplier_sku_code,
            "供应商 SKU 编码不能为空",
            SKU_CODE_MAX_LEN,
            "供应商 SKU 编码过长",
        )?;
        let source_revision_token =
            normalize_optional_text(data.source_revision_token, "来源版本标识", REVISION_TOKEN_MAX_LEN)?;
        let error_text = normalize_optional_text(data.error_text, "错误说明", ERROR_TEXT_MAX_LEN)?;
        ensure_result_consistency(
            data.result,
            error_text.is_some(),
            data.supplier_catalog_sku_id.is_some(),
        )?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            supplier_catalog_intake_batch_id: data.supplier_catalog_intake_batch_id,
            row_no: data.row_no,
            supplier_sku_code,
            source_revision_token,
            classification: data.classification,
            result: data.result,
            error_text,
            supplier_catalog_sku_id: data.supplier_catalog_sku_id,
        })
    }
}

/// 校验行号从 1 开始。
///
/// # 参数
/// * `row_no` - 来源行号
///
/// # 错误
/// 行号为零时返回错误。
fn ensure_row_no(row_no: u32) -> Result<()> {
    if row_no == 0 {
        return Err(Error::from("行号必须从 1 开始"));
    }
    Ok(())
}

/// 校验结果一致性：失败必填错误说明，成功不得携带错误说明且可携带 SKU。
///
/// # 参数
/// * `result` - 处理结果
/// * `has_error_text` - 是否填写错误说明
/// * `has_sku` - 是否携带处理到的供应商 SKU
///
/// # 错误
/// 失败缺少错误说明，或成功携带错误说明时返回错误。
fn ensure_result_consistency(result: IntakeItemResult, has_error_text: bool, has_sku: bool) -> Result<()> {
    match result {
        IntakeItemResult::Failed => {
            if !has_error_text {
                return Err(Error::from("失败明细必须填写错误说明"));
            }
            if has_sku {
                return Err(Error::from("失败明细不得携带处理到的供应商 SKU"));
            }
        }
        IntakeItemResult::Success => {
            if has_error_text {
                return Err(Error::from("成功明细不得填写错误说明"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        IntakeBatchStatus, IntakeItemClassification, IntakeItemResult, SupplierCatalogIntakeBatch,
        SupplierCatalogIntakeBatchData, SupplierCatalogIntakeItem, SupplierCatalogIntakeItemData,
    };
    use crate::ids::{
        FileAssetId, SupplierAccountId, SupplierApiConnectionId, SupplierCatalogIntakeBatchId,
        SupplierCatalogIntakeItemId, SupplierCatalogSkuId,
    };
    use crate::supplier_catalog::types::CatalogSourceType;

    fn batch_data() -> SupplierCatalogIntakeBatchData {
        SupplierCatalogIntakeBatchData {
            source_type: CatalogSourceType::Excel,
            supplier_id: SupplierAccountId::new("sup-1"),
            source_reference: " excel-2026-08-01.xlsx ".to_string(),
            source_connection_id: None,
            file_asset_id: Some(FileAssetId::new("file-1")),
        }
    }

    fn item_data() -> SupplierCatalogIntakeItemData {
        SupplierCatalogIntakeItemData {
            supplier_catalog_intake_batch_id: SupplierCatalogIntakeBatchId::new("scib-1"),
            row_no: 1,
            supplier_sku_code: " SKU-001 ".to_string(),
            source_revision_token: Some("v1".to_string()),
            classification: IntakeItemClassification::New,
            result: IntakeItemResult::Success,
            error_text: None,
            supplier_catalog_sku_id: Some(SupplierCatalogSkuId::new("scs-1")),
        }
    }

    #[test]
    fn batch_new_trims_and_rejects_illegal_connection() {
        let batch =
            SupplierCatalogIntakeBatch::new(SupplierCatalogIntakeBatchId::new("scib-1"), batch_data())
                .unwrap();
        assert_eq!(batch.source_reference, "excel-2026-08-01.xlsx");
        assert_eq!(batch.status, IntakeBatchStatus::Pending);

        let illegal = SupplierCatalogIntakeBatchData {
            source_type: CatalogSourceType::Manual,
            source_connection_id: Some(SupplierApiConnectionId::new("conn-1")),
            ..batch_data()
        };
        assert!(
            SupplierCatalogIntakeBatch::new(SupplierCatalogIntakeBatchId::new("scib-2"), illegal).is_err()
        );

        let empty_ref = SupplierCatalogIntakeBatchData {
            source_reference: "   ".to_string(),
            ..batch_data()
        };
        assert!(
            SupplierCatalogIntakeBatch::new(SupplierCatalogIntakeBatchId::new("scib-3"), empty_ref).is_err()
        );
    }

    #[test]
    fn batch_status_update_requires_error_pairing() {
        let mut batch =
            SupplierCatalogIntakeBatch::new(SupplierCatalogIntakeBatchId::new("scib-1"), batch_data())
                .unwrap();
        assert!(batch
            .update(IntakeBatchStatus::Failed, Some("解析失败".to_string()))
            .is_ok());
        assert_eq!(batch.error_text.as_deref(), Some("解析失败"));

        let mut ok_batch =
            SupplierCatalogIntakeBatch::new(SupplierCatalogIntakeBatchId::new("scib-2"), batch_data())
                .unwrap();
        ok_batch.update(IntakeBatchStatus::Completed, None).unwrap();

        let mut bad_batch =
            SupplierCatalogIntakeBatch::new(SupplierCatalogIntakeBatchId::new("scib-3"), batch_data())
                .unwrap();
        assert!(bad_batch.update(IntakeBatchStatus::Failed, None).is_err());
        assert!(bad_batch
            .update(IntakeBatchStatus::Completed, Some("多余".to_string()))
            .is_err());
    }

    #[test]
    fn item_new_validates_row_no_and_result_consistency() {
        let item =
            SupplierCatalogIntakeItem::new(SupplierCatalogIntakeItemId::new("scii-1"), item_data()).unwrap();
        assert_eq!(item.supplier_sku_code, "SKU-001");
        assert_eq!(item.classification, IntakeItemClassification::New);

        let zero_row = SupplierCatalogIntakeItemData {
            row_no: 0,
            ..item_data()
        };
        assert!(
            SupplierCatalogIntakeItem::new(SupplierCatalogIntakeItemId::new("scii-2"), zero_row).is_err()
        );

        let failed_without_error = SupplierCatalogIntakeItemData {
            result: IntakeItemResult::Failed,
            error_text: None,
            supplier_catalog_sku_id: None,
            ..item_data()
        };
        assert!(SupplierCatalogIntakeItem::new(
            SupplierCatalogIntakeItemId::new("scii-3"),
            failed_without_error,
        )
        .is_err());

        let success_with_error = SupplierCatalogIntakeItemData {
            result: IntakeItemResult::Success,
            error_text: Some("x".to_string()),
            ..item_data()
        };
        assert!(SupplierCatalogIntakeItem::new(
            SupplierCatalogIntakeItemId::new("scii-4"),
            success_with_error,
        )
        .is_err());
    }

    #[test]
    fn intake_enums_expose_labels_and_codes() {
        assert_eq!(IntakeBatchStatus::Processing.label(), "处理中");
        assert_eq!(IntakeItemClassification::Stopped.label(), "停止供应");
        assert_eq!(IntakeItemResult::Failed.as_str(), "FAILED");
        assert_eq!(
            serde_json::to_string(&IntakeBatchStatus::Failed).unwrap(),
            "\"FAILED\""
        );
        assert_eq!(
            serde_json::to_string(&IntakeItemClassification::Unchanged).unwrap(),
            "\"UNCHANGED\""
        );
    }
}
