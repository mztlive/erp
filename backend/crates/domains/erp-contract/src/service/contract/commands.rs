//! 合同后续归档与终止；授权、原对象、版本、写入及回包共用原事务。
use application_core::AuditActor;
use erp_permissions::{Permission, contract};
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::{ContractService, conflict_if_stale_version, plan_next_revision};
use crate::dto::contract::{ArchiveContractRevisionRequest, ContractDetailView, TerminateContractRequest};
use crate::entity::contract::Contract;
use crate::repository::ContractExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

impl ContractService {
    /// 签约 PDF 或归档资料更新时，以请求的期望版本为并发边界追加不可变修订，并切换稳定合同的当前版本指针。
    /// 同一写事务完成实际合同授权、追加版本、审计和完整回包；无权、缺单、版本冲突、输入非法或仓储失败时拒绝。
    pub async fn archive_contract_revision(
        &self,
        id: &str,
        req: ArchiveContractRevisionRequest,
        actor: &AuditActor,
    ) -> Result<ContractDetailView> {
        req.validate()?;

        let (service, id, actor) = (self.clone(), id.to_owned(), actor.clone());
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.archive_stage(&id, req, &actor, executor).await })
            })
            .await
    }

    // 在原写事务先授权，再核对原合同版本并读取历史最大修订号；追加修订、切换当前指针和写审计后用同一执行器装配结果。
    async fn archive_stage(
        &self,
        id: &str,
        req: ArchiveContractRevisionRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ContractDetailView> {
        let contract = self.command_contract(actor, &contract::ARCHIVE_REVISION, id, executor).await?;
        conflict_if_stale_version(contract.matches_version(req.version))?;

        let revision_no = self
            .db
            .contract_revisions()
            .latest_revision_no(&contract.base.id.clone().into(), executor)
            .await?;
        let mut planned = plan_next_revision(contract, req, revision_no.unwrap_or(0))?;

        let audit =
            self.audit.resource_log(actor.clone(), "contract.archive_revision", "contract", id.into())?;
        self.db
            .contract()
            .archive_contract_revision(&mut planned.contract, &planned.revision, executor)
            .await?;
        self.audit.persist(&audit, executor).await?;

        self.detail_view(planned.contract, executor).await
    }

    /// 合同终止时保持既有销售引用与历史修订，只在原授权事务变更对应稳定合同的状态并记录审计。
    /// 无权、缺单、版本冲突、状态非法或数据库失败时不交付完整合同响应。
    pub async fn terminate_contract(
        &self,
        id: &str,
        req: TerminateContractRequest,
        actor: &AuditActor,
    ) -> Result<ContractDetailView> {
        req.validate()?;

        let (service, id, actor) = (self.clone(), id.to_owned(), actor.clone());
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.terminate_stage(&id, req, &actor, executor).await })
            })
            .await
    }

    // 终止命令在写事务内先检查对象授权，再核对版本和状态并写回审计。命令回包沿原执行器装配，不附加详情菜单资格。
    async fn terminate_stage(
        &self,
        id: &str,
        req: TerminateContractRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ContractDetailView> {
        let mut contract = self.command_contract(actor, &contract::TERMINATE, id, executor).await?;
        conflict_if_stale_version(contract.matches_version(req.version))?;
        contract.terminate(actor.id())?;

        let audit = self.audit.resource_log(actor.clone(), "contract.terminate", "contract", id.into())?;
        self.db.contracts().update(&mut contract, executor).await?;
        self.audit.persist(&audit, executor).await?;

        self.detail_view(contract, executor).await
    }

    // 命令处理中先按固定动作检查实际合同对象授权，再读取原合同供版本及业务状态检查；授权失败不继续读取。
    async fn command_contract(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Contract> {
        self.data_scope.require_with(actor.clone(), permission, id, executor).await?;
        self.db
            .contracts()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("合同不存在".into()))
    }
}

#[cfg(test)]
mod tests;
