//! Rig OpenAI Responses API 到合同全文提取 Port 的适配。
mod diagnostics;
mod output;
mod rejection;
#[cfg(test)]
mod tests;
mod transport;

use std::time::{Duration, Instant};

use async_trait::async_trait;
use diagnostics::Diagnostics;
use erp_contract::entity::recognition::{ContractExtraction, ImportFailure, OcrDocument};
use erp_contract::ports::recognition::ContractExtractor;
use rig_agent::completion::{PromptError, StructuredOutputError};
use rig_agent::extractor::ExtractorBuilder;
use rig_core::http_client::{Error as HttpError, HttpClientExt};
use rig_core::providers::openai::OpenAIConfig;
use rig_core::{ErrorKind, ProviderError};
use serde_json::json;
use tokio::time::timeout;
use transport::{BoundedHttp, Failure};

type Result<T> = std::result::Result<T, ImportFailure>;
const PROMPT_VERSION: &str = "contract-v4";

/// 每个任务冻结配置；仅允许 Rig submit 输出，无业务工具及主数据访问权限，不自动重试。
pub struct OpenAiContractExtractor {
    provider_id: String,
    base_url: String,
    api_key: String,
    model: String,
    timeout: Duration,
    max_output_tokens: u64,
}

impl OpenAiContractExtractor {
    /// 装配提取端，不执行外部请求。
    /// # 参数
    /// * `provider_id` - 已由 SafeConfig 校验的非敏感供应商/网关标识，冻结到提取证据。
    /// * `base_url` / `api_key` / `model` - 已由 SafeConfig 校验的兼容服务配置。
    /// * `timeout_seconds` / `max_output_tokens` - 已校验的超时与输出 token 上限。
    /// # 返回
    /// 实现 ContractExtractor 的适配器。
    /// # 错误
    /// 无；客户端构造与请求失败由任务记录。
    pub fn new(
        provider_id: String,
        base_url: String,
        api_key: String,
        model: String,
        timeout_seconds: u64,
        max_output_tokens: u64,
    ) -> Self {
        Self {
            provider_id,
            base_url,
            api_key,
            model,
            timeout: Duration::from_secs(timeout_seconds),
            max_output_tokens,
        }
    }

    #[tracing::instrument(name = "contract_ai", skip_all, fields(
        provider_id = %self.provider_id, page_count = document.pages.len(),
        model = %self.model, base_url = %self.base_url,
        protocol = "responses", extraction_method = "rig_extractor", output_tool = "submit",
        prompt_version = PROMPT_VERSION,
        timeout_seconds = self.timeout.as_secs(), max_output_tokens = self.max_output_tokens
    ))]
    async fn extract_with(
        &self,
        document: &OcrDocument,
        http: impl HttpClientExt + 'static,
        diagnostics: Diagnostics,
    ) -> Result<ContractExtraction> {
        let count = u32::try_from(document.pages.len()).map_err(|_| invalid_output())?;
        document.validate(count)?;
        let prompt = serde_json::to_string(&document.pages).map_err(|_| invalid_output())?;
        let client = OpenAIConfig::new(self.api_key.clone())
            .with_base_url(self.base_url.trim_end_matches('/'))
            .connect(http);
        let mut model = client.responses(&self.model);
        model.wire = model.wire.with_strict_tools();
        let extractor = ExtractorBuilder::<output::Output>::new(model)
            .append_preamble(include_str!("prompt.txt"))
            .max_tokens(self.max_output_tokens)
            .additional_params(json!({"store": false, "parallel_tool_calls": false}))
            .add_hook(diagnostics.clone())
            .retries(0)
            .build();

        let extraction = extractor.extract(prompt).record_content_telemetry(true);
        let start = Instant::now();
        tracing::info!(event = "contract_ai_started", "开始提取合同字段");
        // SDK 与适配器共享任务上下文，错误正文由诊断字段完整记录。
        let (result, timeout_source) = match timeout(self.timeout, extraction).await {
            Err(_) => {
                let (error, source) = deadline_failure(&diagnostics);
                (Err(error), source)
            },
            Ok(Err(error)) => {
                diagnostics.extraction_error(&error);
                let (error, source) = extraction_failure(&error, &diagnostics);
                (Err(error), source)
            },
            Ok(Ok(response)) => {
                let reported = response
                    .completion_calls
                    .last()
                    .and_then(|call| call.raw["model"].as_str())
                    .unwrap_or("unreported");
                let result = output::convert(response.output, &self.model, reported, &self.provider_id)
                    .map_err(|error| {
                        diagnostics.output_error(&error);
                        invalid_output()
                    });
                (result, None)
            },
        };
        diagnostics.finish(start, &result, timeout_source);
        result
    }
}

#[async_trait]
impl ContractExtractor for OpenAiContractExtractor {
    async fn extract(&self, document: &OcrDocument) -> Result<ContractExtraction> {
        let diagnostics = Diagnostics::default();
        let http = BoundedHttp::new(self.timeout, diagnostics.clone()).map_err(|_| unavailable())?;
        self.extract_with(document, http, diagnostics).await
    }
}

fn extraction_failure(
    error: &StructuredOutputError,
    diagnostics: &Diagnostics,
) -> (ImportFailure, Option<&'static str>) {
    if let StructuredOutputError::PromptError(PromptError::CompletionError(error)) = error {
        return provider_failure(error);
    }
    if let Some(status) = error.provider_response_status().filter(|status| !status.is_success()) {
        return status_failure(status.as_u16());
    }
    // Rig runtime 的 ErrorReport 不保留自定义 Rust 错误类型，传输边界保留原始分类。
    if let Some(failure) = diagnostics.failure() {
        return transport_failure(failure);
    }
    if let StructuredOutputError::PromptError(PromptError::Report(report)) = error
        && report.kind == ErrorKind::Http
    {
        return (unavailable(), None);
    }
    (invalid_output(), None)
}

fn invalid_output() -> ImportFailure {
    ImportFailure::new("AI_INVALID_OUTPUT", "字段提取结果不完整或格式无效，请重试或联系管理员")
}

fn unavailable() -> ImportFailure {
    ImportFailure::new("AI_UNAVAILABLE", "字段提取服务暂不可用，请稍后重试")
}

fn timeout_error() -> ImportFailure {
    ImportFailure::new("AI_TIMEOUT", "合同字段提取超时，请稍后重试")
}

fn provider_failure(error: &ProviderError) -> (ImportFailure, Option<&'static str>) {
    if let Some(status) = error.provider_response_status() {
        // Responses 解码失败也可能保留 HTTP 200；这不是供应商拒绝请求。
        return if status.is_success() { (invalid_output(), None) } else { status_failure(status.as_u16()) };
    }
    let ProviderError::Http(error) = error else { return (invalid_output(), None) };
    if let HttpError::Instance(inner) = error.as_ref()
        && let Some(failure) = inner.downcast_ref::<Failure>()
    {
        return transport_failure(*failure);
    }
    (unavailable(), None)
}

fn status_failure(status: u16) -> (ImportFailure, Option<&'static str>) {
    let error = match status {
        401 | 403 => ImportFailure::new("AI_UNAUTHORIZED", "字段提取服务凭据无效或未授权，请联系管理员"),
        429 => ImportFailure::new("AI_THROTTLED", "字段提取服务限流，请稍后重试"),
        408 | 504 => return (timeout_error(), Some("upstream_http")),
        500..=599 => unavailable(),
        _ => ImportFailure::new("AI_REJECTED", "字段提取请求被拒绝，请检查模型、上下文容量及结构化输出支持"),
    };
    (error, None)
}

fn deadline_failure(diagnostics: &Diagnostics) -> (ImportFailure, Option<&'static str>) {
    // 已知 HTTP 拒绝优先；诊断正文未读完不能把 400/401 等改报成 AI_TIMEOUT。
    if let Some(status) = diagnostics.status().filter(|status| !(200..300).contains(status)) {
        return status_failure(status);
    }
    (timeout_error(), Some("application_deadline"))
}

fn transport_failure(failure: Failure) -> (ImportFailure, Option<&'static str>) {
    match failure {
        Failure::Timeout(source) => (timeout_error(), Some(source.as_str())),
        Failure::ResponseSize => (invalid_output(), None),
        Failure::Unavailable | Failure::Unsupported => (unavailable(), None),
    }
}
