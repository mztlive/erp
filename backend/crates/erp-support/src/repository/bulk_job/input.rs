//! 后台任务私有输入记录；按任务物理主键读取，使用 MongoDB 隐式唯一 `_id` 索引。

use erp_core::ids::BackgroundJobId;
use mongodb::Database;
use mongodb::bson::spec::BinarySubtype;
use mongodb::bson::{Binary, Bson, Document, doc};
use persistence_core::{Executor, Result as RepositoryResult, mongo_ops};

use super::BulkJobRepository;
use crate::repository::extensions::BulkJobExt;
use crate::{Error, Result};

/// 私有输入的独立容量边界；达到 1 MiB 必须使用原文件存储链路。
pub const BACKGROUND_JOB_INPUT_LIMIT: usize = 1024 * 1024;

/// 已校验的有界任务输入；内容不进入公开任务或明细响应。
pub struct BackgroundJobInput {
    format: String,
    bytes: Vec<u8>,
}

impl BackgroundJobInput {
    /// 构造仅供任务执行使用的有界私有输入。
    ///
    /// # 参数
    /// 格式标识及已序列化字节；格式版本由拥有业务的 Process 解释。
    /// # 返回
    /// 返回不执行 I/O 的输入记录。
    /// # 错误
    /// 格式为空、过长或字节达到独立容量边界时拒绝。
    pub fn new(format: impl Into<String>, bytes: Vec<u8>) -> Result<Self> {
        let format = format.into();
        if format.is_empty() || format.len() > 128 || bytes.len() >= BACKGROUND_JOB_INPUT_LIMIT {
            return Err(Error::ValidationError("后台任务私有输入格式或大小非法".into()));
        }
        Ok(Self { format, bytes })
    }
}

impl BulkJobRepository<'_> {
    /// 在任务登记的原事务中保存私有输入，不另开事务或覆盖既有记录。
    ///
    /// # 参数
    /// 任务身份、已校验输入以及创建任务和明细使用的同一执行器。
    /// # 返回
    /// 输入与任务、明细一起提交时可见；由 `_id` 唯一索引保护一任务一输入。
    /// # 错误
    /// 唯一键竞争、事务或数据库写入失败时保留仓储错误。
    pub async fn create_job_input(
        &self,
        job_id: &BackgroundJobId,
        input: &BackgroundJobInput,
        executor: &mut dyn Executor,
    ) -> RepositoryResult<()> {
        let record = input_document(job_id, input);
        mongo_ops::insert_one(
            &self.db.collection::<Document>(<Database as BulkJobExt>::BACKGROUND_JOB_INPUTS),
            &record,
            executor,
        )
        .await
    }

    /// 读取指定任务的私有有界输入；不通过普通任务列表、详情或导出暴露内容。
    ///
    /// # 参数
    /// 任务身份、期望格式以及调用方执行器。
    /// # 返回
    /// 格式及二进制容量有效时返回字节；无记录或损坏记录返回 `None`，供旧任务回退。
    /// # 错误
    /// 数据库读取失败时保留仓储错误，不把持久化失败解释为不存在。
    pub async fn job_input(
        &self,
        job_id: &BackgroundJobId,
        format: &str,
        executor: &mut dyn Executor,
    ) -> RepositoryResult<Option<Vec<u8>>> {
        let record = mongo_ops::find_one(
            &self.db.collection::<Document>(<Database as BulkJobExt>::BACKGROUND_JOB_INPUTS),
            doc! { "_id": job_id.as_ref() },
            executor,
        )
        .await?;
        Ok(record.and_then(|record| input_bytes(record, format)))
    }
}

/// 二进制 BSON 保持字节内容与独立容量上限，不复制成逐字节数组。
fn input_document(job_id: &BackgroundJobId, input: &BackgroundJobInput) -> Document {
    doc! { "_id": job_id.as_ref(), "format": &input.format,
    "payload": Bson::Binary(Binary { subtype: BinarySubtype::Generic, bytes: input.bytes.clone() }) }
}

/// 私有格式、二进制类型或容量损坏时不尝试解释为业务输入。
fn input_bytes(mut record: Document, format: &str) -> Option<Vec<u8>> {
    if record.get_str("format").ok()? != format {
        return None;
    }
    match record.remove("payload")? {
        Bson::Binary(binary)
            if binary.subtype == BinarySubtype::Generic
                && binary.bytes.len() < BACKGROUND_JOB_INPUT_LIMIT =>
        {
            Some(binary.bytes)
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// BSON 记录按任务物理主键存储二进制，格式和内容往返保持原样。
    #[test]
    fn private_input_roundtrips_binary_with_task_identity() {
        let input = BackgroundJobInput::new("format-v1", vec![0, 255, 7]).unwrap();
        let record = input_document(&BackgroundJobId::new("job"), &input);
        assert_eq!(record.get_str("_id").unwrap(), "job");
        assert!(matches!(record.get("payload"), Some(Bson::Binary(_))));
        assert_eq!(input_bytes(record, "format-v1"), Some(vec![0, 255, 7]));
    }

    /// 容量界限前一字节允许私有输入，达到界限整体拒绝，不截断内容。
    #[test]
    fn private_input_capacity_and_format_are_bounded() {
        assert!(BackgroundJobInput::new("format", vec![0; BACKGROUND_JOB_INPUT_LIMIT - 1]).is_ok());
        assert!(BackgroundJobInput::new("format", vec![0; BACKGROUND_JOB_INPUT_LIMIT]).is_err());
        assert!(BackgroundJobInput::new("format", vec![0; BACKGROUND_JOB_INPUT_LIMIT + 1]).is_err());
        assert!(BackgroundJobInput::new("", vec![]).is_err());
        assert!(BackgroundJobInput::new("f".repeat(129), vec![]).is_err());
    }

    /// 旧记录无格式、异格式、错误字节类型和超限损坏均返回兼容回退标记。
    #[test]
    fn legacy_and_corrupt_inputs_are_unavailable() {
        assert!(input_bytes(doc! { "_id": "job" }, "format-v1").is_none());
        assert!(input_bytes(doc! { "format": "format-v1", "payload": [1, 2] }, "format-v1").is_none());
        let input = BackgroundJobInput::new("format-v1", vec![1]).unwrap();
        assert!(input_bytes(input_document(&BackgroundJobId::new("job"), &input), "other").is_none());
        let oversized = doc! { "format": "format-v1", "payload": Bson::Binary(Binary {
            subtype: BinarySubtype::Generic, bytes: vec![0; BACKGROUND_JOB_INPUT_LIMIT],
        }) };
        assert!(input_bytes(oversized, "format-v1").is_none());
        let wrong_binary = doc! { "format": "format-v1", "payload": Bson::Binary(Binary {
            subtype: BinarySubtype::Uuid, bytes: vec![0; 16],
        }) };
        assert!(input_bytes(wrong_binary, "format-v1").is_none());
    }
}
