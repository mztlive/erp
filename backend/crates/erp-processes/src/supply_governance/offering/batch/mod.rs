//! 有界批量供给：先校验全批，逐行复用原事务和持久化回执。
use std::collections::HashSet;

use application_core::AuditActor;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::SupplierOfferingProcess;
use crate::{Error, Result};

mod operations;
#[cfg(test)]
mod tests;

/// 每次批量最多处理的行数。
pub const MAX_BATCH_ROWS: usize = 100;

/// 单批请求，校验模式不执行写入。
#[derive(Debug, Deserialize)]
pub struct BatchRequest<T> {
    /// 仅校验并恢复已完成的行。
    #[serde(default)]
    pub validate_only: bool,
    /// 客户端稳定行标识与完整命令。
    pub rows: Vec<BatchRow<T>>,
}

/// 一行批量命令。
#[derive(Debug, Deserialize)]
pub struct BatchRow<T> {
    /// 用于把结果匹配回表格的稳定标识。
    pub row_id: String,
    /// 原单条用例的完整参数。
    pub input: T,
}

/// 已有供给的定向命令。
#[derive(Debug, Clone, Deserialize)]
pub struct Targeted<T> {
    /// 目标供给主键。
    pub offering_id: String,
    /// 条款或可供更新参数。
    pub command: T,
}

/// 批量逐行结果。
#[derive(Debug, Serialize)]
pub struct BatchResult {
    /// 与请求顺序一致的结果；一行失败不抹去其他行回执。
    pub rows: Vec<RowResult>,
}

/// 一行当前处理结果。
#[derive(Debug, Serialize)]
pub struct RowResult {
    /// 请求中的行标识。
    pub row_id: String,
    /// 当前处理状态。
    pub status: RowStatus,
    /// 可展示的错误或处理说明。
    pub message: Option<String>,
    /// 已完成命令的原始结果。
    pub result: Option<Value>,
}

/// 客户端必须将 Unknown 行锁定为原始命令后重试。
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RowStatus {
    /// 校验通过，但还未写入。
    Ready,
    /// 已写入，或读取到持久化回执。
    Succeeded,
    /// 校验失败，整批未开始新的写入。
    Invalid,
    /// 单行写入明确失败，可以修正后重新校验。
    Failed,
    /// 结果不能确认，须原命令恢复，禁止改内容或换键。
    Unknown,
}

/// 批量编排所需的窄用例接口。
#[async_trait]
pub trait BatchOperation: Send + Sync + 'static {
    /// 返回批内唯一业务身份。
    fn identity(&self) -> String;
    /// 返回批量创建的供应商；更新命令返回空。
    fn supplier(&self) -> Option<&str> {
        None
    }
    /// 取原命令键，用于批内去重与主体隔离。
    fn key(&mut self) -> &mut String;
    /// 校验权限、资格、版本和字段；已执行时返回持久化结果。
    async fn prepare(&self, process: &SupplierOfferingProcess, actor: &AuditActor) -> Result<Option<Value>>;
    /// 调用拥有原子事务的单条用例。
    async fn execute(self, process: &SupplierOfferingProcess, actor: &AuditActor) -> Result<Value>;
}

impl SupplierOfferingProcess {
    /// 校验整批并按行执行供给操作。
    ///
    /// # 参数
    /// * `request` - 最多 100 行同类操作；创建限一个供应商
    /// * `actor` - 当前操作人，命令键按该主体隔离
    ///
    /// # 返回
    /// 返回每行校验、成功、失败或待确认结果。
    ///
    /// # 错误
    /// 空批、超限、重复行/业务身份/命令键或混合供应商时拒绝整批。
    pub async fn batch<T: BatchOperation>(
        &self,
        mut request: BatchRequest<T>,
        actor: &AuditActor,
    ) -> Result<BatchResult> {
        validate_envelope(&mut request, actor.id())?;
        run_batch(&ProcessRunner { process: self, actor }, request).await
    }
}

/// 验证容器约束，并将客户端命令键限定到当前主体。
fn validate_envelope<T: BatchOperation>(request: &mut BatchRequest<T>, actor: &str) -> Result<()> {
    if request.rows.is_empty() || request.rows.len() > MAX_BATCH_ROWS {
        return Err(Error::ValidationError("每批请选择 1–100 行".into()));
    }
    let mut rows = HashSet::new();
    let mut identities = HashSet::new();
    let mut keys = HashSet::new();
    let mut suppliers = HashSet::new();
    for row in &mut request.rows {
        if row.row_id.trim().is_empty() || row.row_id.len() > 200 || !rows.insert(row.row_id.clone()) {
            return Err(Error::ValidationError("行标识为空或重复".into()));
        }
        if !identities.insert(row.input.identity()) {
            return Err(Error::ValidationError("同一批次存在重复的供给，请合并后提交".into()));
        }
        if let Some(supplier) = row.input.supplier() {
            suppliers.insert(supplier.to_owned());
        }
        let key = row.input.key();
        if key.trim().is_empty() || key.len() > 200 || !keys.insert(key.clone()) {
            return Err(Error::ValidationError("提交标识为空或重复，请重新打开配置".into()));
        }
        *key = format!("batch:{}:{actor}:{key}", actor.len());
    }
    if suppliers.len() > 1 {
        return Err(Error::ValidationError("批量新增每次只能选择一个供应商".into()));
    }
    Ok(())
}

/// 批量编排依赖的窄执行端口，单元测试无需构造数据库客户端。
#[async_trait]
trait BatchRunner<T>: Sync {
    /// 预检一个完整命令。
    async fn prepare(&self, input: &T) -> Result<Option<Value>>;
    /// 提交一个完整命令。
    async fn execute(&self, input: T) -> Result<Value>;
}
struct ProcessRunner<'a> {
    process: &'a SupplierOfferingProcess,
    actor: &'a AuditActor,
}
#[async_trait]
impl<T: BatchOperation> BatchRunner<T> for ProcessRunner<'_> {
    /// 接入单条供给预检。
    async fn prepare(&self, input: &T) -> Result<Option<Value>> {
        input.prepare(self.process, self.actor).await
    }
    /// 接入单条供给事务。
    async fn execute(&self, input: T) -> Result<Value> {
        input.execute(self.process, self.actor).await
    }
}
/// 全批预检通过后逐行提交，回执恢复优先于新写入。
async fn run_batch<T: Send + Sync>(
    runner: &impl BatchRunner<T>,
    request: BatchRequest<T>,
) -> Result<BatchResult> {
    let mut results = Vec::with_capacity(request.rows.len());
    for row in &request.rows {
        let (status, message, result) = match runner.prepare(&row.input).await {
            Ok(Some(value)) => (RowStatus::Succeeded, None, Some(value)),
            Ok(None) => (RowStatus::Ready, None, None),
            Err(error) => (RowStatus::Invalid, Some(safe_message(&error)), None),
        };
        results.push(RowResult { row_id: row.row_id.clone(), status, message, result });
    }
    let blocked = results.iter().any(|row| row.status == RowStatus::Invalid);
    if !request.validate_only && !blocked {
        for (row, result) in request.rows.into_iter().zip(&mut results) {
            if result.status == RowStatus::Succeeded {
                continue;
            }
            match runner.execute(row.input).await {
                Ok(value) => {
                    result.status = RowStatus::Succeeded;
                    result.result = Some(value);
                },
                Err(error) => {
                    result.status = failure_status(&error);
                    result.message = Some(safe_message(&error));
                },
            }
        }
    }
    Ok(BatchResult { rows: results })
}

/// 保守区分明确拒绝与提交结果不确定，后者只允许原命令恢复。
fn failure_status(error: &Error) -> RowStatus {
    match error {
        Error::ValidationError(_)
        | Error::BusinessLogicError(_)
        | Error::ConflictError(_)
        | Error::NotFound(_)
        | Error::Forbidden(_)
        | Error::Unauthenticated(_)
        | Error::Logic(_)
        | Error::TransientTransaction(_) => RowStatus::Failed,
        _ => RowStatus::Unknown,
    }
}

/// 不向逐行结果泄露数据库或服务内部细节。
fn safe_message(error: &Error) -> String {
    match error {
        Error::ValidationError(message)
        | Error::BusinessLogicError(message)
        | Error::ConflictError(message)
        | Error::NotFound(message)
        | Error::Forbidden(message)
        | Error::Unauthenticated(message) => message.clone(),
        Error::Logic(_) => "字段不符合供给规则，请检查金额、数量和日期".into(),
        Error::TransientTransaction(_) => "数据同时被修改，请重试本行".into(),
        _ => "暂时无法确认处理结果，请保留当前内容并重试确认".into(),
    }
}
