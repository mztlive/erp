//! 履约成功命令查证与同事务回执写入。
use application_core::CommandReceipt;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::command_receipt::FulfillmentCommandReceipt;
use crate::repository::FulfillmentCommandExt;
use crate::{Error, Result};

/// 本领域命令回执服务，不开事务、不读取审计。
pub struct FulfillmentCommandReceiptService {
    db: Database,
}
impl FulfillmentCommandReceiptService {
    /// 装配履约命令回执读写入口；使用本域数据库，不开启事务或读取审计正文。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    /// 原客户验收命令重试或提交结果未知时，用调用方的事务或只读执行器按原命令身份查证。
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
    /// 首次完成客户验收命令时，沿用用例事务保存原命令、强类型结果 ID 和成功审计 ID。
    /// 回执与业务写入及成功审计须由入口共同提交；目录或结果非法、唯一键冲突或写入失败时返回错误。
    pub async fn save_resource(
        &self,
        command: &CommandReceipt,
        resource_id: String,
        audit_event_id: String,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let receipt = FulfillmentCommandReceipt::resource(command, resource_id, audit_event_id)?;
        self.db.command_receipts().create(&receipt, executor).await.map_err(Error::from)?;
        Ok(())
    }
}
