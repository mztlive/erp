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
    // 逐页渲染前，将已检查不超过 20 MiB 的原始 PDF 写入私有临时目录，暂不启动 pdftoppm。
    // 目录由渲染上下文持有；空文件、临时目录创建或写盘失败均报 OCR_RENDER_FAILED。
    pub(super) async fn create(pdf: &[u8], program: &Path) -> Result<Self> {
        if pdf.is_empty() {
            return Err(render_error());
        }
        let directory = tempfile::tempdir().map_err(|_| render_error())?;
        let path = directory.path().join("contract.pdf");
        tokio::fs::write(&path, pdf).await.map_err(|_| render_error())?;
        Ok(Self { _directory: directory, path, program: program.into() })
    }

    // OCR 按页调用 pdftoppm，以相同起止页码获取一张大小受限且具有 PNG 文件头的图片，供后续完整解码。
    // 单页限时 20 秒；超时和启动失败分别报 OCR_RENDER_TIMEOUT、OCR_RENDER_UNAVAILABLE，空图或超限报 OCR_IMAGE_SIZE，
    // 非 PNG 或进程失败报 OCR_RENDER_FAILED。
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
