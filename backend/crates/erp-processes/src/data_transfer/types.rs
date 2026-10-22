use application_core::AuditActor;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use erp_core::AccountKind;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Error, Result};

/// 服务端允许执行的任务目录，不接受任意 URL、权限或文件路径。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferKind {
    ProductPoolExcel,
    SupplierQuoteArchive,
    SalesOrders,
    PurchaseOrders,
    Contracts,
    Customers,
    InventoryBalance,
    InventoryMovement,
    InventoryAdjustment,
    InventoryReservation,
    SupplierOrders,
    Receivables,
    SupplierPayables,
    InvoiceRequests,
    ProfitLoss,
    CustomerQualityCurrent,
    CustomerQualityHistory,
    MasterData,
    InquiryPreview,
    InquiryCreationPreview,
    SupplierQuotePreview,
    SupplyBatch,
    PortalBatch,
}
impl TransferKind {
    /// 返回个人任务列表中的业务名称。
    /// # 参数
    /// 无。
    /// # 返回
    /// 不包含业务敏感数据的固定中文名称。
    /// # 错误
    /// 无。
    pub fn label(self) -> &'static str {
        match self {
            Self::ProductPoolExcel => "商品池 Excel 导出",
            Self::SupplierQuoteArchive => "供应商报价包导出",
            Self::SalesOrders => "销售单导出",
            Self::PurchaseOrders => "采购单导出",
            Self::Contracts => "合同导出",
            Self::Customers => "客户目录导出",
            Self::InventoryBalance => "库存余额导出",
            Self::InventoryMovement => "库存流水导出",
            Self::InventoryAdjustment => "库存调整导出",
            Self::InventoryReservation => "销售预占导出",
            Self::SupplierOrders => "供应商订单导出",
            Self::Receivables => "客户往来导出",
            Self::SupplierPayables => "供应商往来导出",
            Self::InvoiceRequests => "开票申请导出",
            Self::ProfitLoss => "经营盈亏导出",
            Self::CustomerQualityCurrent => "客户质量当前口径导出",
            Self::CustomerQualityHistory => "客户质量历史口径导出",
            Self::MasterData => "基础资料导出",
            Self::InquiryPreview => "询价导入预览",
            Self::InquiryCreationPreview => "新建询价导入预览",
            Self::SupplierQuotePreview => "供应商报价导入预览",
            Self::SupplyBatch => "供给批量处理",
            Self::PortalBatch => "门户批量处理",
        }
    }

    /// 判断任务是否生成文件。
    /// # 参数
    /// 无。
    /// # 返回
    /// 导出返回 true；导入解析或批量写入返回 false。
    /// # 错误
    /// 无。
    pub fn is_export(self) -> bool {
        !matches!(
            self,
            Self::InquiryPreview
                | Self::InquiryCreationPreview
                | Self::SupplierQuotePreview
                | Self::SupplyBatch
                | Self::PortalBatch
        )
    }
}

/// 创建任务只接收业务参数与稳定操作号，任务身份和进度由服务端产生。
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TransferRequest {
    pub request_id: String,
    pub kind: TransferKind,
    pub input: Value,
}

#[derive(Deserialize, Serialize)]
pub(super) struct Envelope {
    pub kind: TransferKind,
    pub input: Value,
    pub account: String,
    pub account_kind: AccountKind,
}
impl Envelope {
    pub fn actor(&self, id: &str, request: &str) -> Result<AuditActor> {
        Ok(AuditActor::new(id.into(), self.account.clone(), self.account_kind)
            .with_request_id(Some(request.into()))?)
    }
}

/// 私有产物；授权依据与文件字节不进入普通任务列表或详情。
#[derive(Serialize, Deserialize)]
pub struct TransferResult {
    pub file_name: String,
    pub content_type: String,
    pub content_base64: String,
    pub authorization: Value,
    #[serde(default)]
    pub metadata: Value,
}
impl TransferResult {
    /// 构造有界文件结果，禁止不安全文件名。
    /// # 参数
    /// 文件名、MIME、完整文件字节与业务范围依据。
    /// # 返回
    /// 可加密持久化的结果。
    /// # 错误
    /// 文件超出 64 MiB 或名称包含路径及控制字符时拒绝。
    pub fn file(name: String, mime: &str, bytes: Vec<u8>, authorization: Value) -> Result<Self> {
        if bytes.len() > 64 * 1024 * 1024
            || name.is_empty()
            || name.len() > 240
            || name.chars().any(|c| c.is_control() || c == '/' || c == '\\')
        {
            return Err(Error::ValidationError("文件过大或文件名无效，请缩小处理范围".into()));
        }
        Ok(Self {
            file_name: name,
            content_type: mime.into(),
            content_base64: STANDARD.encode(bytes),
            authorization,
            metadata: Value::Null,
        })
    }
    /// 解码受控结果字节。
    /// # 参数
    /// 无。
    /// # 返回
    /// 原始文件内容。
    /// # 错误
    /// 持久化内容损坏时拒绝。
    pub fn bytes(&self) -> Result<Vec<u8>> {
        STANDARD.decode(&self.content_base64).map_err(|_| Error::Internal("任务结果损坏，请重新提交".into()))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    #[test]
    fn private_file_rejects_paths_and_roundtrips_binary() {
        for name in ["../secret", "a/b", "a\\b", "a\n.csv", ""] {
            assert!(TransferResult::file(name.into(), "text/csv", vec![], Value::Null).is_err());
        }
        let file = TransferResult::file(
            "结果.xlsx".into(),
            "application/octet-stream",
            vec![0, 1, 255],
            Value::Null,
        )
        .unwrap();
        assert_eq!(file.bytes().unwrap(), vec![0, 1, 255]);
    }
    #[test]
    fn unknown_kind_and_caller_identity_are_rejected() {
        assert!(
            serde_json::from_value::<TransferRequest>(
                json!({"request_id":"r","kind":"arbitrary_url","input":{}})
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<TransferRequest>(
                json!({"request_id":"r","kind":"sales_orders","input":{},"requested_by":"other"})
            )
            .is_err()
        );
    }
}
