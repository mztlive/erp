//! 持久化导入导出任务：输入密文归档、CAS 认领、心跳、受控结果读取。
mod business;
mod context;
mod context_input;
mod context_view;
mod csv;
mod execute;
mod imports;
mod master;
mod outcome;
mod portal;
mod quote_import;
mod quotes;
mod recovery;
mod workbook;
mod workbook_access;
pub use business::{BusinessTransfers, TransferReader};
mod files;
mod submit;
mod types;

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
pub use context_view::{TransferContext, TransferResumeState};
use erp_party::SensitiveDataCodec;
use mongodb::Database;
use serde_json::Value;
use storage::S3Storage;
pub use types::{TransferKind, TransferRequest, TransferResult};

use crate::Result;

/// 业务适配器只接受固定任务类型；授权与业务规则沿用原用例。
#[async_trait]
pub trait TransferHandler: Send + Sync {
    /// 校验输入并冻结执行条件。
    /// # 参数
    /// 固定类型、原始参数和当前认证人。
    /// # 返回
    /// 已校验的私有执行输入。
    /// # 错误
    /// 权限、范围、版本或参数无效时拒绝。
    async fn prepare(&self, kind: TransferKind, input: Value, actor: &AuditActor) -> Result<Value>;
    /// 执行已登记的任务，写命令必须沿用输入中的稳定命令身份。
    /// # 参数
    /// 固定类型、已冻结输入和原提交人。
    /// # 返回
    /// 文件或导入预览结果及结果读取所需授权依据。
    /// # 错误
    /// 业务拒绝或执行失败时返回安全错误。
    async fn execute(&self, kind: TransferKind, input: &Value, actor: &AuditActor) -> Result<TransferResult>;
    /// 读取产物前后重验当前权限和结果绑定的数据范围。
    /// # 参数
    /// 固定类型、原执行输入、结果授权依据和当前认证人。
    /// # 返回
    /// 仍可读取时返回空值。
    /// # 错误
    /// 撤权、范围或版本变化时拒绝。
    async fn authorize_result(
        &self,
        kind: TransferKind,
        input: &Value,
        authorization: &Value,
        actor: &AuditActor,
    ) -> Result<()>;
    /// 返回当前权限允许公开的业务续办上下文。
    /// # 参数
    /// 固定类型、私有输入、可选原结果和当前认证人。
    /// # 返回
    /// 白名单业务引用、真实续办状态与有效期。
    /// # 错误
    /// 身份、权限或业务读取失败时拒绝。
    async fn context(
        &self,
        kind: TransferKind,
        input: &Value,
        result: Option<&Value>,
        actor: &AuditActor,
    ) -> Result<TransferContext>;
    /// 仅查询原批量命令回执，不启动新的业务写入。
    /// # 参数
    /// 批量类型、原私有输入和当前认证人。
    /// # 返回
    /// 当前已确认回执及仍待确认的结果。
    /// # 错误
    /// 非批量任务、越权或恢复查询失败时拒绝。
    async fn recover(&self, kind: TransferKind, input: &Value, actor: &AuditActor) -> Result<Value>;
}

/// 可由独立有界 worker 与 HTTP 入口共享的导入导出流程。
#[derive(Clone)]
pub struct DataTransferProcess {
    db: Database,
    storage: S3Storage,
    codec: Arc<SensitiveDataCodec>,
    handler: Arc<dyn TransferHandler>,
}
impl DataTransferProcess {
    /// 装配存储、密文编解码与业务执行适配器。
    /// # 参数
    /// 数据库、对象存储、敏感数据密钥与已装配业务适配器。
    /// # 返回
    /// 不执行 I/O 的流程实例。
    /// # 错误
    /// 无。
    pub fn new(
        db: Database,
        storage: S3Storage,
        codec: Arc<SensitiveDataCodec>,
        handler: Arc<dyn TransferHandler>,
    ) -> Self {
        Self { db, storage, codec, handler }
    }
}
