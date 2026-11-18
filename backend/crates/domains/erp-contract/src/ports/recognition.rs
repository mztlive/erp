//! 供应商接入点。默认不接任何供应商，不返回模拟成功结果。
use async_trait::async_trait;

use crate::entity::recognition::{ContractExtraction, ImportFailure, OcrDocument};

/// OCR 适配器必须识别全部页面，并保留空白及不可读页。
#[async_trait]
pub trait ContractOcr: Send + Sync {
    /// 自动归档流程将受控 PDF 及本地校验的页数交给 OCR，返回按页排序的全文，保留空白及不可读页。
    /// 未配置、超时或识别失败返回 `ImportFailure`；不得在日志或返回中泄漏正文、供应商响应或密钥。
    async fn recognize(&self, pdf: &[u8], page_count: u32) -> Result<OcrDocument, ImportFailure>;
}

/// AI 仅提取有页码和引文的原文值，不负责匹配 ERP 身份。
#[async_trait]
pub trait ContractExtractor: Send + Sync {
    /// OCR 完成后由字段提取端口读取完整文档，返回带页码和引文的类型化原文，显式标记正文、附表或签章页冲突。
    /// 不猜测缺值、自行选冲突值或匹配 ERP 身份，不把原文当模型指令；未配置、超时、输出非法或提取失败时返回 `ImportFailure`。
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
