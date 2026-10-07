//! Rig OpenAI Chat Completions 到合同全文提取 Port 的适配。
mod output;
#[cfg(test)]
mod tests;
mod transport;

use std::time::Duration;

use async_trait::async_trait;
use erp_contract::entity::recognition::{ContractExtraction, ImportFailure, OcrDocument};
use erp_contract::ports::recognition::ContractExtractor;
use rig_core::ProviderError;
use rig_core::completion::CompletionRequest;
use rig_core::http_client::{Error as HttpError, HttpClientExt};
use rig_core::providers::openai::OpenAIConfig;
use tokio::time::timeout;
use tracing::instrument::WithSubscriber;
use tracing::subscriber::NoSubscriber;
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

    async fn extract_with(
        &self,
        document: &OcrDocument,
        http: impl HttpClientExt + 'static,
    ) -> Result<ContractExtraction> {
        let count = u32::try_from(document.pages.len()).map_err(|_| invalid_output())?;
        document.validate(count)?;
        let prompt = serde_json::to_string(&document.pages).map_err(|_| invalid_output())?;
        let request = CompletionRequest::new(prompt)
            .preamble(include_str!("prompt.txt"))
            .max_tokens(self.max_output_tokens)
            .output_schema(output::schema()?)
            .record_content_telemetry(false);
        let client = OpenAIConfig::new(self.api_key.clone())
            .with_base_url(self.base_url.trim_end_matches('/'))
            .connect(http);
        let model = client.chat(&self.model);
        // SDK 内部错误/追踪不得记录合同正文、模型输出或服务端错误负载。
        let response = timeout(self.timeout, model.call(request).with_subscriber(NoSubscriber::default()))
            .await
            .map_err(|_| ImportFailure::new("AI_TIMEOUT", "合同字段提取超时，请稍后重试"))?
            .map_err(provider_error)?;
        output::decode(response, &self.model, &self.provider_id)
    }
}

#[async_trait]
impl ContractExtractor for OpenAiContractExtractor {
    async fn extract(&self, document: &OcrDocument) -> Result<ContractExtraction> {
        let http = BoundedHttp::new(self.timeout)
            .map_err(|_| ImportFailure::new("AI_UNAVAILABLE", "字段提取服务暂不可用，请稍后重试"))?;
        self.extract_with(document, http).await
    }
}

fn invalid_output() -> ImportFailure {
    ImportFailure::new("AI_INVALID_OUTPUT", "字段提取结果不完整或格式无效，请重试或联系管理员")
}

fn provider_error(error: ProviderError) -> ImportFailure {
    if let Some(status) = error.provider_response_status() {
        return match status.as_u16() {
            401 | 403 => ImportFailure::new("AI_UNAUTHORIZED", "字段提取服务凭据无效或未授权，请联系管理员"),
            429 => ImportFailure::new("AI_THROTTLED", "字段提取服务限流，请稍后重试"),
            408 | 504 => ImportFailure::new("AI_TIMEOUT", "合同字段提取超时，请稍后重试"),
            500..=599 => ImportFailure::new("AI_UNAVAILABLE", "字段提取服务暂不可用，请稍后重试"),
            _ => ImportFailure::new(
                "AI_REJECTED",
                "字段提取请求被拒绝，请检查模型、上下文容量及结构化输出支持",
            ),
        };
    }
    if let ProviderError::Http(error) = error {
        if let HttpError::Instance(inner) = error.as_ref() {
            match inner.downcast_ref::<Failure>() {
                Some(Failure::Timeout) => {
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
