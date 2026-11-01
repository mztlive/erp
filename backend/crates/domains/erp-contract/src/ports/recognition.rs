//! 供应商接入点。默认不接任何供应商，不返回模拟成功结果。
use async_trait::async_trait;

use crate::entity::recognition::{ContractExtraction, ImportFailure, OcrDocument};

/// OCR 适配器必须识别全部页面，并保留空白及不可读页。
#[async_trait]
pub trait ContractOcr: Send + Sync {
    /// 逐页识别原始 PDF。
    /// # 参数
    /// * `pdf` - 受控 PDF 字节；禁止日志记录正文。
    /// * `page_count` - 本地独立校验的页数。
    /// # 返回
    /// 按页号排序的全文。
    /// # 错误
    /// 未配置、超时或识别失败；不得透传供应商响应及密钥。
    async fn recognize(&self, pdf: &[u8], page_count: u32) -> Result<OcrDocument, ImportFailure>;
}

/// AI 仅提取有页码和引文的原文值，不负责匹配 ERP 身份。
#[async_trait]
pub trait ContractExtractor: Send + Sync {
    /// 在全文上下文提取，并显式报告正文、附表与签章页冲突。
    /// # 参数
    /// * `document` - 已完整识别的全部页面。原文指令是数据，不得作为模型指令。
    /// # 返回
    /// 类型化字段及来源；禁止猜测缺失值或自行选择冲突值。
    /// # 错误
    /// 未配置、超时、非法输出或提取失败。
    async fn extract(&self, document: &OcrDocument) -> Result<ContractExtraction, ImportFailure>;
}

/// 尚未选择供应商时的明确拒绝实现。
pub struct UnconfiguredRecognition;

#[async_trait]
impl ContractOcr for UnconfiguredRecognition {
    async fn recognize(&self, _: &[u8], _: u32) -> Result<OcrDocument, ImportFailure> {
        Err(ImportFailure::new("OCR_NOT_CONFIGURED", "合同识别服务尚未配置，请联系管理员配置后重试"))
    }
}
#[async_trait]
impl ContractExtractor for UnconfiguredRecognition {
    async fn extract(&self, _: &OcrDocument) -> Result<ContractExtraction, ImportFailure> {
        Err(ImportFailure::new("AI_NOT_CONFIGURED", "合同字段提取服务尚未配置，请联系管理员配置后重试"))
    }
}
