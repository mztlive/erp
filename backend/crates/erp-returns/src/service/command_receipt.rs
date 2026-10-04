//! 退货逆向成功命令查证与同事务回执写入。
use application_core::CommandReceipt;
use mongodb::Database;
use persistence_core::{Error as PersistenceError, Executor};

use crate::entity::command_receipt::ReturnsCommandReceipt;
use crate::indexes::COMMAND_RECEIPT_ID_INDEX;
use crate::repository::ReturnsCommandExt;
use crate::{Error, Result};

/// 本领域命令回执服务，不开事务、不读取审计。
pub struct ReturnsCommandReceiptService {
    db: Database,
}
impl ReturnsCommandReceiptService {
    /// 绑定拥有领域数据库句柄。
    ///
    /// # 参数
    /// * `db` - 数据库句柄。
    /// # 返回
    /// 返回领域命令回执服务。
    /// # 错误
    /// 无。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    /// 读取已提交原命令的结果引用。
    ///
    /// # 参数
    /// * `command` - 原身份与载荷。
    /// * `executor` - 调用方事务或只读查证执行器。
    /// # 返回
    /// 命中返回原对象 ID，无命中返回 None。
    /// # 错误
    /// 异载荷、损坏或数据库失败时返回错误。
    pub async fn committed_resource_id(
        &self,
        command: &CommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        let receipt = self.db.command_receipts().find_by_id_including_deleted(command.id(), executor).await?;
        receipt.map(|receipt| receipt.resource_id(command)).transpose()
    }
    /// 保存新鲜执行的强类型结果；业务事实与审计由入口统一事务提交。
    ///
    /// # 参数
    /// * `command` - 原身份与载荷。
    /// * `resource_id` - 正式结果对象 ID。
    /// * `audit_event_id` - 关联成功审计 ID。
    /// * `executor` - 拥有用例事务执行器。
    /// # 返回
    /// 写入成功时返回空结果。
    /// # 错误
    /// 目录/结果非法、唯一冲突或写入失败时返回错误。
    pub async fn save_resource(
        &self,
        command: &CommandReceipt,
        resource_id: String,
        audit_event_id: String,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let receipt = ReturnsCommandReceipt::resource(command, resource_id, audit_event_id)?;
        self.db.command_receipts().create(&receipt, executor).await.map_err(receipt_write_error)?;
        Ok(())
    }
}
/// 已知命令唯一索引冲突保留可只读恢复的错误分类。
fn receipt_write_error(error: PersistenceError) -> Error {
    if error.duplicate_index_name() == Some(COMMAND_RECEIPT_ID_INDEX) {
        Error::ReceiptDuplicate(error)
    } else {
        Error::from(error)
    }
}
