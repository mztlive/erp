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
    /// 装配退货逆向命令回执读写入口；使用本域数据库，不开启事务或读取审计正文。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    /// 原退款和收付款冲正命令重试或提交结果未知时，用调用方的事务或只读执行器按原命令身份查证。
    /// 仅身份与载荷匹配时返回已提交结果的对象 ID；没有记录时返回 `None`，载荷冲突、记录损坏和读取失败均返回错误。
    /// 调用方仍须执行本次授权；查证不会再写业务事实或成功审计。
    pub async fn committed_resource_id(
        &self,
        command: &CommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        let receipt = self.db.command_receipts().find_by_id_including_deleted(command.id(), executor).await?;
        receipt.map(|receipt| receipt.resource_id(command)).transpose()
    }
    /// 首次完成退款和收付款冲正命令时，沿用用例事务保存原命令、强类型结果 ID 和成功审计 ID。
    /// 回执与业务写入及成功审计须由入口共同提交；目录或结果非法、唯一键冲突或写入失败时返回错误。
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
// 仅将命令回执 ID 的已知唯一索引冲突映射为 `ReceiptDuplicate`，供入口只读查证并发胜者；其他错误按原类型转换。
fn receipt_write_error(error: PersistenceError) -> Error {
    if error.duplicate_index_name() == Some(COMMAND_RECEIPT_ID_INDEX) {
        Error::ReceiptDuplicate(error)
    } else {
        Error::from(error)
    }
}
