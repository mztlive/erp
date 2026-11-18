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
    /// 任务登记前构造仅执行端可见的私有输入。格式标识由所属业务流程解释，须非空且最多 128 字节；内容达到 1 MiB 时拒绝，不截断。
    pub fn new(format: impl Into<String>, bytes: Vec<u8>) -> Result<Self> {
        let format = format.into();
        if format.is_empty() || format.len() > 128 || bytes.len() >= BACKGROUND_JOB_INPUT_LIMIT {
            return Err(Error::ValidationError("任务所需资料格式不正确或内容过大，请检查后重新提交".into()));
        }
        Ok(Self { format, bytes })
    }
}

impl BulkJobRepository<'_> {
    /// 执行任务时按任务 ID 和预期格式沿当前执行器严格读取提交时的私有资料。没有记录返回 `None`；格式、BSON 二进制类型或容量损坏，以及读取失败均返回错误。
    pub async fn strict_job_input(
        &self,
        job_id: &BackgroundJobId,
        format: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<Vec<u8>>> {
        let record = mongo_ops::find_one(
            &self.db.collection::<Document>(<Database as BulkJobExt>::BACKGROUND_JOB_INPUTS),
            doc! { "_id": job_id.as_ref() },
            executor,
        )
        .await?;
        record
            .map(|record| {
                input_bytes(record, format).ok_or_else(|| {
                    Error::ValidationError("无法读取任务提交时的资料，请核对任务详情并联系管理员".into())
                })
            })
            .transpose()
    }

    /// 登记任务和明细时沿原事务插入私有输入，以任务 ID 作 `_id`，不另开事务或覆盖旧记录。
    /// 唯一键竞争、事务失败或数据库写入失败时保留仓储错误。
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

    /// 有兼容存储链路的任务恢复时，按任务 ID 和预期格式读取私有字节，不向列表、详情、明细或导出返回。
    /// 无记录或记录损坏返回 `None` 供旧链路回退；数据库读取失败原样传播，不能视为缺记录。
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

// 登记私有输入时用任务 ID 作物理主键、用 Generic BSON Binary 保存有效载荷，避免把字节编码成普通数组。
fn input_document(job_id: &BackgroundJobId, input: &BackgroundJobInput) -> Document {
    doc! { "_id": job_id.as_ref(), "format": &input.format,
    "payload": Bson::Binary(Binary { subtype: BinarySubtype::Generic, bytes: input.bytes.clone() }) }
}

// 解包私有输入时核对版本格式、BSON 二进制类型和容量；任一不匹配即返回 `None`，由对应读取入口决定报错或回退。
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

    // 验证BSON 记录按任务物理主键存储二进制，格式和内容往返保持原样。
    #[test]
    fn private_input_roundtrips_binary_with_task_identity() {
        let input = BackgroundJobInput::new("format-v1", vec![0, 255, 7]).unwrap();
        let record = input_document(&BackgroundJobId::new("job"), &input);
        assert_eq!(record.get_str("_id").unwrap(), "job");
        assert!(matches!(record.get("payload"), Some(Bson::Binary(_))));
        assert_eq!(input_bytes(record, "format-v1"), Some(vec![0, 255, 7]));
    }

    // 验证容量界限前一字节允许私有输入，达到界限整体拒绝，不截断内容。
    #[test]
    fn private_input_capacity_and_format_are_bounded() {
        assert!(BackgroundJobInput::new("format", vec![0; BACKGROUND_JOB_INPUT_LIMIT - 1]).is_ok());
        assert!(BackgroundJobInput::new("format", vec![0; BACKGROUND_JOB_INPUT_LIMIT]).is_err());
        assert!(BackgroundJobInput::new("format", vec![0; BACKGROUND_JOB_INPUT_LIMIT + 1]).is_err());
        assert!(BackgroundJobInput::new("", vec![]).is_err());
        assert!(BackgroundJobInput::new("f".repeat(129), vec![]).is_err());
    }

    // 验证旧记录无格式、异格式、错误字节类型和超限损坏均返回兼容回退标记。
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
