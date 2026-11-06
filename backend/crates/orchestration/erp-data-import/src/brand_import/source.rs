//! 读取加密源文件并重新核对内容指纹、提交人和任务登记。
use application_core::command::CommandFingerprint;
use erp_catalog::dto::BrandImportJobRequest;
use erp_support::BackgroundJob;

use super::BrandImportProcess;
use crate::{Error, Result};
impl BrandImportProcess {
    /// 读取并解密任务源内容，只供执行器和已校验归属的失败下载调用。解密失败不返回源字节。
    ///
    /// # 参数
    /// * `job` - 已登记的品牌导入任务。
    ///
    /// # 返回
    /// 返回校验通过且指纹属于该任务的导入请求。
    ///
    /// # 错误
    /// 源路径非法、读取或解密失败、内容损坏、校验失败、指纹不属于该任务，或与任务登记不一致时返回错误。
    pub(super) async fn source(&self, job: &BackgroundJob) -> Result<BrandImportJobRequest> {
        let source = job
            .domain_job_id
            .as_deref()
            .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| Error::Internal("导入任务缺少源数据".into()))?;
        let bytes = self
            .storage
            .read(super::submit::source_key(source))
            .await
            .map_err(|_| Error::Internal("读取品牌导入源文件失败".into()))?;
        let encoded = std::str::from_utf8(&bytes).map_err(|_| Error::Internal("导入源数据损坏".into()))?;
        let json = self.codec.decrypt(encoded)?;
        let request: BrandImportJobRequest =
            serde_json::from_str(&json).map_err(|_| Error::Internal("导入源数据损坏".into()))?;
        request.key()?;
        let expected = CommandFingerprint::from_parts([
            hex::encode(self.codec.fingerprint_key()),
            job.requested_by.clone(),
            json,
        ]);
        if expected.digest_hex() != source {
            return Err(Error::Forbidden("导入源数据不属于该任务".into()));
        }
        let (proposed, _) = super::submit::build_job(&request, &job.requested_by, source)?;
        super::submit::ensure_replay(job, &proposed)?;
        Ok(request)
    }
}
