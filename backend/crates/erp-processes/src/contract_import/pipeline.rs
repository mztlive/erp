//! 外部调用只在数据库事务外执行；供应商错误统一由适配器脱敏。
use std::sync::Arc;
use std::time::Duration;

use erp_contract::entity::recognition::{ContractExtraction, ImportFailure, OcrDocument};
use erp_contract::ports::recognition::{ContractExtractor, ContractOcr, UnconfiguredRecognition};
use tokio::time::timeout;

/// 仅保存 trait 对象，AppState 按 SafeConfig 快照装配供应商。
#[derive(Clone)]
pub struct RecognitionProviders {
    pub ocr: Arc<dyn ContractOcr>,
    pub extractor: Arc<dyn ContractExtractor>,
}
impl Default for RecognitionProviders {
    fn default() -> Self {
        Self { ocr: Arc::new(UnconfiguredRecognition), extractor: Arc::new(UnconfiguredRecognition) }
    }
}

impl RecognitionProviders {
    /// 完整执行 OCR、全文提取和证据校验，保留已完成阶段用于失败诊断。
    /// # 参数
    /// * `pdf` / `page_count` - 原文件及独立页数。
    /// # 返回
    /// OCR、提取结果和可选业务失败。
    /// # 错误
    /// 错误作为第三项返回；总超时 180 秒，禁止隐式重试。
    pub async fn run(&self, pdf: &[u8], page_count: u32) -> RecognitionAttempt {
        let mut attempt = RecognitionAttempt::default();
        let result = timeout(Duration::from_secs(180), async {
            let document = self.ocr.recognize(pdf, page_count).await?;
            document.validate(page_count)?;
            attempt.ocr = Some(document.clone());
            let extraction = self.extractor.extract(&document).await?;
            // 提取结果大小必须有界；不得保存任意供应商负载。
            if serde_json::to_vec(&extraction).map_or(true, |bytes| bytes.len() > 256_000) {
                return Err(ImportFailure::new("EXTRACTION_TOO_LARGE", "字段提取结果异常，请联系管理员"));
            }
            attempt.extraction = Some(extraction.clone());
            extraction.validate(&document)?;
            Ok::<(), ImportFailure>(())
        })
        .await;
        attempt.failure = match result {
            Ok(result) => result.err(),
            Err(_) => Some(ImportFailure::new("RECOGNITION_TIMEOUT", "合同识别超时，请稍后重试")),
        };
        attempt
    }
}

#[derive(Default)]
pub struct RecognitionAttempt {
    pub ocr: Option<OcrDocument>,
    pub extraction: Option<ContractExtraction>,
    pub failure: Option<ImportFailure>,
}
