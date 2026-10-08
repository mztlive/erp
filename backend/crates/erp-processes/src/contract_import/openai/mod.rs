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
use rig_core::ProviderError;
use rig_core::completion::CompletionRequest;
use rig_core::http_client::{Error as HttpError, HttpClientExt};
use rig_core::providers::openai::OpenAIConfig;
use serde_json::json;
use tokio::time::timeout;
use transport::{BoundedHttp, Failure};

type Result<T> = std::result::Result<T, ImportFailure>;

/// 每个任务冻结配置；模型无工具、无主数据访问权限，不自动重试。
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
        protocol = "responses", response_format = "json_schema", strict = true,
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
        let request = CompletionRequest::new(prompt)
            .preamble(include_str!("prompt.txt"))
            .max_tokens(self.max_output_tokens)
            .output_schema(output::schema()?)
            .additional_params(json!({"store": false}))
            .record_content_telemetry(true);
        let client = OpenAIConfig::new(self.api_key.clone())
            .with_base_url(self.base_url.trim_end_matches('/'))
            .connect(http);
        let model = client.responses(&self.model);
        let start = Instant::now();
        tracing::info!(event = "contract_ai_started", "开始提取合同字段");
        // SDK 与适配器共享任务上下文，错误正文由诊断字段完整记录。
        let (result, timeout_source) = match timeout(self.timeout, model.call(request)).await {
            Err(_) => deadline_failure(&diagnostics),
            Ok(Err(error)) => {
                diagnostics.error(&error);
                (Err(provider_error(&error)), timeout_source(&error))
            },
            Ok(Ok(response)) => (output::decode(response, &self.model, &self.provider_id), None),
        };
        diagnostics.finish(start, &result, timeout_source);
        result
    }
}

#[async_trait]
impl ContractExtractor for OpenAiContractExtractor {
    async fn extract(&self, document: &OcrDocument) -> Result<ContractExtraction> {
        let diagnostics = Diagnostics::default();
        let http = BoundedHttp::new(self.timeout, diagnostics.clone())
            .map_err(|_| ImportFailure::new("AI_UNAVAILABLE", "字段提取服务暂不可用，请稍后重试"))?;
        self.extract_with(document, http, diagnostics).await
    }
}

fn invalid_output() -> ImportFailure {
    ImportFailure::new("AI_INVALID_OUTPUT", "字段提取结果不完整或格式无效，请重试或联系管理员")
}

fn provider_error(error: &ProviderError) -> ImportFailure {
    if let Some(status) = error.provider_response_status() {
        // Responses 解码失败也可能保留 HTTP 200；这不是供应商拒绝请求。
        return if status.is_success() { invalid_output() } else { status_error(status.as_u16()) };
    }
    if let ProviderError::Http(error) = error {
        if let HttpError::Instance(inner) = error.as_ref() {
            match inner.downcast_ref::<Failure>() {
                Some(Failure::Timeout(_)) => {
                    return ImportFailure::new("AI_TIMEOUT", "合同字段提取超时，请稍后重试");
                },
                Some(Failure::ResponseSize) => return invalid_output(),
                _ => {},
            }
        }
        return ImportFailure::new("AI_UNAVAILABLE", "字段提取服务暂不可用，请稍后重试");
    }
    invalid_output()
}

fn status_error(status: u16) -> ImportFailure {
    match status {
        401 | 403 => ImportFailure::new("AI_UNAUTHORIZED", "字段提取服务凭据无效或未授权，请联系管理员"),
        429 => ImportFailure::new("AI_THROTTLED", "字段提取服务限流，请稍后重试"),
        408 | 504 => ImportFailure::new("AI_TIMEOUT", "合同字段提取超时，请稍后重试"),
        500..=599 => ImportFailure::new("AI_UNAVAILABLE", "字段提取服务暂不可用，请稍后重试"),
        _ => ImportFailure::new("AI_REJECTED", "字段提取请求被拒绝，请检查模型、上下文容量及结构化输出支持"),
    }
}

fn deadline_failure(diagnostics: &Diagnostics) -> (Result<ContractExtraction>, Option<&'static str>) {
    // 已知 HTTP 拒绝优先；诊断正文未读完不能把 400/401 等改报成 AI_TIMEOUT。
    if let Some(status) = diagnostics.status().filter(|status| !(200..300).contains(status)) {
        return (Err(status_error(status)), matches!(status, 408 | 504).then_some("upstream_http"));
    }
    (Err(ImportFailure::new("AI_TIMEOUT", "合同字段提取超时，请稍后重试")), Some("application_deadline"))
}

fn timeout_source(error: &ProviderError) -> Option<&'static str> {
    if error.provider_response_status().is_some_and(|status| matches!(status.as_u16(), 408 | 504)) {
        return Some("upstream_http");
    }
    if let ProviderError::Http(error) = error
        && let HttpError::Instance(inner) = error.as_ref()
        && let Some(Failure::Timeout(source)) = inner.downcast_ref::<Failure>()
    {
        return Some(source.as_str());
    }
    None
}
