//! 阿里云图片 OCR 到合同逐页 OCR Port 的组合层适配。
mod image;
mod render;

use std::path::PathBuf;

use aliyun_ocr::{Client, Credentials, Error as OcrError};
use async_trait::async_trait;
use erp_contract::entity::recognition::{ImportFailure, OcrDocument, OcrPage, pdf_page_count};
use erp_contract::ports::recognition::ContractOcr;
use render::PdfFile;

type Result<T> = std::result::Result<T, ImportFailure>;

/// 逐页转图并调用阿里云；原 PDF、供应商响应及凭据均不得输出日志。
pub struct AliyunContractOcr {
    credentials: Credentials,
    pdftoppm_path: PathBuf,
}
impl AliyunContractOcr {
    /// 从当前 SafeConfig 快照装配 OCR 适配器。
    /// # 参数
    /// * `credentials` - 阿里云凭据
    /// * `pdftoppm_path` - 管理员配置的 Poppler 执行文件
    /// # 返回
    /// 延迟创建 HTTP 客户端的适配器，不访问网络。
    /// # 错误
    /// 无；配置及运行环境错误由识别任务记录。
    pub fn new(credentials: Credentials, pdftoppm_path: PathBuf) -> Self {
        Self { credentials, pdftoppm_path }
    }
}

#[async_trait]
impl ContractOcr for AliyunContractOcr {
    async fn recognize(&self, pdf: &[u8], page_count: u32) -> Result<OcrDocument> {
        if pdf.len() > 20 * 1024 * 1024 {
            return Err(ImportFailure::new("OCR_PDF_SIZE", "合同 PDF 超过识别大小限制"));
        }
        // 独立验证，防止适配器被其他调用方传入截断页数。
        if pdf_page_count(pdf)? != page_count {
            return Err(ImportFailure::new("PAGE_COVERAGE", "合同页数不一致，请重新上传完整文件"));
        }
        let client = Client::new(self.credentials.clone()).map_err(provider_error)?;
        let pdf = PdfFile::create(pdf, &self.pdftoppm_path).await?;
        collect_pages(&LivePages { client, pdf }, page_count).await
    }
}

#[async_trait]
trait PageSource: Send + Sync {
    async fn recognize(&self, number: u32) -> Result<(OcrPage, String)>;
}
struct LivePages {
    client: Client,
    pdf: PdfFile,
}
#[async_trait]
impl PageSource for LivePages {
    async fn recognize(&self, number: u32) -> Result<(OcrPage, String)> {
        let image = self.pdf.page(number).await?;
        let image = tokio::task::spawn_blocking(move || image::inspect(image))
            .await
            .map_err(|_| ImportFailure::new("OCR_RENDER_FAILED", "页面图片处理失败，请重试"))??;
        if image.blank {
            return Ok((OcrPage { number, text: String::new(), blank: true, readable: true }, String::new()));
        }
        let page = self.client.recognize_image(&image.bytes).await.map_err(provider_error)?;
        // API 不提供可靠的空白页判定；零文字不得伪装成成功空白页。
        let readable = !page.text.trim().is_empty();
        Ok((OcrPage { number, text: page.text, blank: false, readable }, page.version))
    }
}

async fn collect_pages(source: &dyn PageSource, page_count: u32) -> Result<OcrDocument> {
    if !(1..=200).contains(&page_count) {
        return Err(ImportFailure::new("PAGE_COVERAGE", "合同须包含 1 至 200 页"));
    }
    let mut document =
        OcrDocument { provider: "aliyun-ocr".into(), version: String::new(), pages: Vec::new() };
    let mut bytes = 0_usize;
    for number in 1..=page_count {
        let (page, version) = source.recognize(number).await.map_err(|mut error| {
            error.page = Some(number);
            error
        })?;
        bytes = bytes.saturating_add(page.text.len());
        if page.number != number
            || !page.readable
            || page.blank != page.text.trim().is_empty()
            || page.text.len() > 100_000
            || bytes > 2_000_000
        {
            let mut error = ImportFailure::new(
                "PAGE_UNREADABLE",
                "页面未识别到完整文字，不能确认空白页，请上传清晰的合同",
            );
            error.page = Some(number);
            return Err(error);
        }
        if !version.is_empty() && document.version.is_empty() {
            document.version = version;
        } else if !version.is_empty() && document.version != version {
            return Err(ImportFailure::new(
                "OCR_VERSION_CHANGED",
                "识别服务版本发生变化，请重新识别整份合同",
            ));
        }
        document.pages.push(page);
    }
    if document.version.is_empty() {
        document.version = "2021-07-07;local-white-pages".into();
    }
    document.validate(page_count)?;
    Ok(document)
}

fn provider_error(error: OcrError) -> ImportFailure {
    let (code, message) = match error {
        OcrError::Configuration => ("OCR_CONFIG_INVALID", "阿里云 OCR 配置无效，请联系管理员"),
        OcrError::Authorization => ("OCR_UNAUTHORIZED", "阿里云 OCR 凭据无效或未授权，请联系管理员"),
        OcrError::Throttled => ("OCR_THROTTLED", "阿里云 OCR 请求限流，请稍后重试"),
        OcrError::Timeout => ("OCR_TIMEOUT", "阿里云 OCR 请求超时，请稍后重试"),
        OcrError::Transport | OcrError::Unavailable => ("OCR_UNAVAILABLE", "阿里云 OCR 暂不可用，请稍后重试"),
        OcrError::ImageSize => ("OCR_IMAGE_SIZE", "合同页面图片超过 10 MB，请调整扫描文件"),
        OcrError::Rejected => ("OCR_REJECTED", "阿里云 OCR 拒绝识别，请检查文件及服务开通状态"),
        OcrError::InvalidResponse | OcrError::ResponseSize => {
            ("OCR_INVALID_RESPONSE", "阿里云 OCR 返回结果无效，请联系管理员")
        },
    };
    ImportFailure::new(code, message)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    struct FakePages {
        calls: Mutex<Vec<u32>>,
        fail_at: Option<u32>,
        empty_at: Option<u32>,
        blank_at: Option<u32>,
    }
    #[async_trait]
    impl PageSource for FakePages {
        async fn recognize(&self, number: u32) -> Result<(OcrPage, String)> {
            self.calls.lock().unwrap().push(number);
            if self.fail_at == Some(number) {
                return Err(provider_error(OcrError::Throttled));
            }
            let text = if self.empty_at == Some(number) || self.blank_at == Some(number) {
                String::new()
            } else {
                format!("第{number}页合同原文")
            };
            Ok((
                OcrPage { number, text, blank: self.blank_at == Some(number), readable: true },
                "test-version".into(),
            ))
        }
    }
    #[tokio::test]
    async fn preserves_every_page_and_never_retries_or_skips_failure() {
        let source = FakePages { calls: Mutex::new(vec![]), fail_at: None, empty_at: None, blank_at: None };
        let document = collect_pages(&source, 3).await.unwrap();
        document.validate(3).unwrap();
        assert_eq!(*source.calls.lock().unwrap(), [1, 2, 3]);
        assert_eq!(document.pages[2].text, "第3页合同原文");
        let source =
            FakePages { calls: Mutex::new(vec![]), fail_at: Some(2), empty_at: None, blank_at: None };
        let failure = collect_pages(&source, 3).await.unwrap_err();
        assert_eq!(failure.code, "OCR_THROTTLED");
        assert_eq!(failure.page, Some(2));
        assert_eq!(*source.calls.lock().unwrap(), [1, 2]);
    }
    #[tokio::test]
    async fn refuses_empty_ocr_instead_of_claiming_blank_page() {
        let source =
            FakePages { calls: Mutex::new(vec![]), fail_at: None, empty_at: Some(1), blank_at: None };
        assert_eq!(collect_pages(&source, 2).await.unwrap_err().code, "PAGE_UNREADABLE");
        assert_eq!(*source.calls.lock().unwrap(), [1]);
    }
    #[tokio::test]
    async fn retains_verified_white_pages_with_original_page_numbers() {
        let source =
            FakePages { calls: Mutex::new(vec![]), fail_at: None, empty_at: None, blank_at: Some(2) };
        let document = collect_pages(&source, 3).await.unwrap();
        document.validate(3).unwrap();
        assert!(document.pages[1].blank);
        assert_eq!(document.pages[1].number, 2);
        assert_eq!(document.pages[2].number, 3);
    }
}
