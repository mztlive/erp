//! 独立财务回执的读取与写入，不开启事务、不反查审计。

use application_core::CommandReceipt;
use erp_core::ids::InvoiceId;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::command_receipt::{FinanceCommandReceipt, pick_purchase_invoice_id};
use crate::repository::{FinanceCommandExt, FinanceCommandReceiptRepositoryExt};
use crate::{Error, Result};

/// 财务命令成功结果恢复服务。
pub struct FinanceCommandReceiptService {
    db: Database,
}

impl FinanceCommandReceiptService {
    /// 绑定拥有领域数据库句柄。
    ///
    /// # 参数
    /// * `db` - 数据库句柄。
    /// # 返回
    /// 返回本域回执服务。
    /// # 错误
    /// 无。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 查询已提交的同一登记命令，沿用原发票当前视图恢复语义。
    ///
    /// # 参数
    /// * `command` - 当前请求的原命令身份及载荷。
    /// * `executor` - 首次、事务内或异常查证使用的执行器。
    /// # 返回
    /// 返回原已登记发票 ID；无命中返回 None。
    /// # 错误
    /// 异载荷、回执损坏、重复身份或数据库错误时明确失败。
    pub async fn committed_purchase_invoice_id(
        &self,
        command: &CommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<Option<InvoiceId>> {
        let receipts = self
            .db
            .finance_command_receipts()
            .find_by_candidates(&[command.id().to_string()], executor)
            .await?;
        pick_purchase_invoice_id(command, &receipts)
    }

    /// 保存新鲜执行结果，业务写入、审计和本回执必须复用同一事务。
    ///
    /// # 参数
    /// * `command` - 原命令身份及载荷。
    /// * `invoice_id` - 已登记发票 ID。
    /// * `audit_event_id` - 关联审计 ID。
    /// * `executor` - 拥有用例的事务执行器。
    /// # 返回
    /// 保存成功时返回空结果。
    /// # 错误
    /// 身份/结果不合法、唯一身份冲突或数据库写入失败时返回错误。
    pub async fn save_purchase_invoice(
        &self,
        command: &CommandReceipt,
        invoice_id: InvoiceId,
        audit_event_id: String,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let receipt = FinanceCommandReceipt::purchase_invoice(command, invoice_id, audit_event_id)?;
        self.db.finance_command_receipts().create(&receipt, executor).await?;
        Ok(())
    }

    /// 读取显式目录内资金命令的原结果对象 ID。
    ///
    /// # 参数
    /// * `command` - 原命令身份及载荷。
    /// * `executor` - 调用方事务或只读查证执行器。
    /// # 返回
    /// 返回原对象 ID；无命中返回 None。
    /// # 错误
    /// 异载荷、损坏、重复身份或数据库错误时返回错误。
    pub async fn committed_resource_id(
        &self,
        command: &CommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        let receipts = self
            .db
            .finance_command_receipts()
            .find_by_candidates(&[command.id().to_string()], executor)
            .await?;
        match receipts.as_slice() {
            [] => Ok(None),
            [receipt] => receipt.resource_id(command).map(Some),
            _ => Err(Error::Internal("业务命令收据格式无效".to_string())),
        }
    }

    /// 保存显式目录内资金命令的强类型结果，与业务及成功审计同 Executor。
    ///
    /// # 参数
    /// * `command` - 原命令身份及载荷。
    /// * `resource_id` - 正式结果对象 ID。
    /// * `audit_event_id` - 关联成功审计 ID。
    /// * `executor` - 拥有用例的事务执行器。
    /// # 返回
    /// 保存成功时返回空结果。
    /// # 错误
    /// 目录/结果非法、唯一身份冲突或写入失败时返回错误。
    pub async fn save_resource(
        &self,
        command: &CommandReceipt,
        resource_id: String,
        audit_event_id: String,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let receipt = FinanceCommandReceipt::resource(command, resource_id, audit_event_id)?;
        self.db.finance_command_receipts().create(&receipt, executor).await?;
        Ok(())
    }
}
