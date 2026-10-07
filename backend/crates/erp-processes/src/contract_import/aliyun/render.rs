//! 私有临时 PDF 与有界逐页 PNG 渲染；取消时终止子进程并清理文件。
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use aliyun_ocr::MAX_IMAGE_BYTES;
use erp_contract::entity::recognition::ImportFailure;
use tempfile::TempDir;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::time::timeout;

type Result<T> = std::result::Result<T, ImportFailure>;

pub(super) struct PdfFile {
    _directory: TempDir,
    path: PathBuf,
    program: PathBuf,
}
impl PdfFile {
    pub(super) async fn create(pdf: &[u8], program: &Path) -> Result<Self> {
        if pdf.is_empty() || pdf.len() > 20 * 1024 * 1024 {
            return Err(render_error());
        }
        let directory = tempfile::tempdir().map_err(|_| render_error())?;
        let path = directory.path().join("contract.pdf");
        tokio::fs::write(&path, pdf).await.map_err(|_| render_error())?;
        Ok(Self { _directory: directory, path, program: program.into() })
    }

    pub(super) async fn page(&self, number: u32) -> Result<Vec<u8>> {
        timeout(Duration::from_secs(20), self.render(number))
            .await
            .map_err(|_| ImportFailure::new("OCR_RENDER_TIMEOUT", "合同页面转图超时，请检查 PDF 后重试"))?
    }

    async fn render(&self, number: u32) -> Result<Vec<u8>> {
        // 不经 shell、不使用裁切或隐藏批注选项，输出到受限管道。
        let mut child = Command::new(&self.program)
            .args([
                "-f",
                &number.to_string(),
                "-l",
                &number.to_string(),
                "-singlefile",
                "-scale-to",
                "3200",
                "-png",
            ])
            .arg(&self.path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| {
                ImportFailure::new(
                    "OCR_RENDER_UNAVAILABLE",
                    "PDF 转图服务不可用，请联系管理员检查 Poppler 配置",
                )
            })?;
        let output = child.stdout.take().ok_or_else(render_error)?;
        let bytes = bounded_image(output).await?;
        if !child.wait().await.map_err(|_| render_error())?.success() {
            return Err(render_error());
        }
        if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err(render_error());
        }
        Ok(bytes)
    }
}

async fn bounded_image(reader: impl AsyncRead + Unpin) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take((MAX_IMAGE_BYTES + 1) as u64).read_to_end(&mut bytes).await.map_err(|_| render_error())?;
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
        return Err(ImportFailure::new(
            "OCR_IMAGE_SIZE",
            "合同页面转图为空或超过 10 MB，请调整扫描文件后重试",
        ));
    }
    Ok(bytes)
}
fn render_error() -> ImportFailure {
    ImportFailure::new("OCR_RENDER_FAILED", "合同页面无法转为图片，请检查 PDF 后重试")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn image_reader_rejects_empty_and_limits_stream_bytes() {
        assert!(bounded_image(b"".as_slice()).await.is_err());
        let bytes = vec![1; MAX_IMAGE_BYTES + 500];
        let error = bounded_image(bytes.as_slice()).await.unwrap_err();
        assert_eq!(error.code, "OCR_IMAGE_SIZE");
        assert_eq!(bounded_image(b"image".as_slice()).await.unwrap(), b"image");
    }
}
