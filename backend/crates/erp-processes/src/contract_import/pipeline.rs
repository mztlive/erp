//! 外部调用只在数据库事务外执行；供应商错误统一由适配器脱敏。
use std::sync::Arc;
use std::time::{Duration, Instant};

use erp_contract::entity::recognition::{ContractExtraction, ImportFailure, ImportStage, OcrDocument};
use erp_contract::ports::recognition::{ContractExtractor, ContractOcr, UnconfiguredRecognition};
use tokio::time::timeout;

use super::progress::RecognitionProgress;
use crate::{Error, Result};

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
    /// * `progress` - 开始各阶段前持久化进度；失败时停止后续供应商调用。
    /// # 返回
    /// OCR、提取结果和可选业务失败。
    /// # 错误
    /// 进度持久化失败直接返回；供应商错误保存在 attempt，总超时 180 秒，禁止隐式重试。
    pub(super) async fn run(
        &self,
        pdf: &[u8],
        page_count: u32,
        progress: &mut dyn RecognitionProgress,
    ) -> Result<RecognitionAttempt> {
        let mut attempt = RecognitionAttempt::default();
        let start = Instant::now();
        let mut stage = ImportStage::Ocr;
        let mut stage_start = start;
        tracing::info!(event = "contract_ocr_started", page_count, "开始合同逐页 OCR");
        let result = timeout(Duration::from_secs(180), async {
            progress.advance(stage).await.map_err(StageFailure::Progress)?;
            let document = self.ocr.recognize(pdf, page_count).await?;
            document.validate(page_count)?;
            tracing::info!(
                event = "contract_ocr_finished",
                page_count,
                elapsed_ms = u64::try_from(stage_start.elapsed().as_millis()).unwrap_or(u64::MAX),
                "合同逐页 OCR 完成"
            );
            attempt.ocr = Some(document.clone());
            stage = ImportStage::AiExtract;
            stage_start = Instant::now();
            progress.advance(stage).await.map_err(StageFailure::Progress)?;
            let extraction = self.extractor.extract(&document).await?;
            stage = ImportStage::PreparingReview;
            stage_start = Instant::now();
            progress.advance(stage).await.map_err(StageFailure::Progress)?;
            // 提取结果大小必须有界；不得保存任意供应商负载。
            if serde_json::to_vec(&extraction).map_or(true, |bytes| bytes.len() > 256_000) {
                return Err(
                    ImportFailure::new("EXTRACTION_TOO_LARGE", "字段提取结果异常，请联系管理员").into()
                );
            }
            attempt.extraction = Some(extraction.clone());
            // 识别只生成待确认草稿；缺失、冲突和未匹配字段不阻断任务。
            Ok::<(), StageFailure>(())
        })
        .await;
        attempt.failure = match result {
            Ok(Ok(())) => None,
            Ok(Err(StageFailure::Recognition(failure))) => Some(failure),
            Ok(Err(StageFailure::Progress(error))) => return Err(error),
            Err(_) => Some(ImportFailure::new("RECOGNITION_TIMEOUT", "合同识别超时，请稍后重试")),
        };
        if let Some(failure) = &attempt.failure {
            log_failure(failure, stage, start, stage_start);
        }
        Ok(attempt)
    }
}

#[derive(Default)]
pub struct RecognitionAttempt {
    pub ocr: Option<OcrDocument>,
    pub extraction: Option<ContractExtraction>,
    pub failure: Option<ImportFailure>,
}

// 数据库错误须原样返回，不能伪装成供应商失败或覆盖竞争中的任务。
#[derive(Debug, thiserror::Error)]
enum StageFailure {
    #[error(transparent)]
    Recognition(#[from] ImportFailure),
    #[error(transparent)]
    Progress(Error),
}

fn log_failure(failure: &ImportFailure, stage: ImportStage, start: Instant, stage_start: Instant) {
    tracing::warn!(
        event = "contract_recognition_failed", ?stage, error_code = %failure.code,
        elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
        stage_elapsed_ms = u64::try_from(stage_start.elapsed().as_millis()).unwrap_or(u64::MAX),
        timeout_seconds = 180, "合同识别阶段失败"
    );
}
