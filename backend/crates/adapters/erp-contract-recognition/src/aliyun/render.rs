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
    /// 把 PDF 写入进程私有临时目录，不启动转图。
    ///
    /// # 参数
    /// * `pdf` - 调用方已校验不超过 20 MB 的原文件字节；空文件会被拒绝。
    /// * `program` - `pdftoppm` 可执行文件路径。
    ///
    /// # 返回
    /// 持有临时目录和 PDF 路径的渲染上下文。
    ///
    /// # 错误
    /// 文件为空、临时目录或写盘失败时返回 `OCR_RENDER_FAILED`。
    pub(super) async fn create(pdf: &[u8], program: &Path) -> Result<Self> {
        if pdf.is_empty() {
            return Err(render_error());
        }
        let directory = tempfile::tempdir().map_err(|_| render_error())?;
        let path = directory.path().join("contract.pdf");
        tokio::fs::write(&path, pdf).await.map_err(|_| render_error())?;
        Ok(Self { _directory: directory, path, program: program.into() })
    }

    /// 在 20 秒内把指定页渲染为单张 PNG。
    ///
    /// # 参数
    /// * `number` - 传给 `pdftoppm` 的起止页码。
    ///
    /// # 返回
    /// 以 PNG 魔数开头、且不超过图片大小上限的字节。
    ///
    /// # 错误
    /// 超时返回 `OCR_RENDER_TIMEOUT`；程序不可用返回 `OCR_RENDER_UNAVAILABLE`；空图、超限、非 PNG 或进程失败返回渲染失败或 `OCR_IMAGE_SIZE`。
    pub(super) async fn page(&self, number: u32) -> Result<Vec<u8>> {
        timeout(Duration::from_secs(20), self.render(number)).await.map_err(|_| {
            ImportFailure::new(
                "OCR_RENDER_TIMEOUT",
                "合同页面无法及时读取，请用清晰、体积较小的 PDF 重新上传",
            )
        })?
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
                ImportFailure::new("OCR_RENDER_UNAVAILABLE", "合同页面读取服务暂不可用，请联系管理员")
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
            "合同页面为空或图片过大，请检查扫描件并压缩后重新上传",
        ));
    }
    Ok(bytes)
}
fn render_error() -> ImportFailure {
    ImportFailure::new("OCR_RENDER_FAILED", "合同页面无法读取，请确认 PDF 可正常打开后重新上传")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn private_pdf_file_keeps_bytes_until_drop_and_rejects_empty() {
        let program = Path::new("unused-pdftoppm");
        let error = PdfFile::create(&[], program).await.err().unwrap();
        assert_eq!(error, render_error());

        for bytes in [b"pdf".to_vec(), vec![0; 20 * 1024 * 1024]] {
            let pdf = PdfFile::create(&bytes, program).await.unwrap();
            let path = pdf.path.clone();
            assert_eq!(tokio::fs::read(&path).await.unwrap(), bytes);
            assert_eq!(pdf.program, program);
            drop(pdf);
            assert!(!path.exists());
            assert!(!path.parent().unwrap().exists());
        }
    }

    #[tokio::test]
    async fn image_reader_rejects_empty_and_limits_stream_bytes() {
        assert!(bounded_image(b"".as_slice()).await.is_err());
        let bytes = vec![1; MAX_IMAGE_BYTES + 500];
        let error = bounded_image(bytes.as_slice()).await.unwrap_err();
        assert_eq!(error.code, "OCR_IMAGE_SIZE");
        assert_eq!(bounded_image(b"image".as_slice()).await.unwrap(), b"image");
    }
}
