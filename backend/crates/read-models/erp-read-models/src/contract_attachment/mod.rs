//! 合同附件在同一授权快照核验合同、不可变修订和文件资产后交付。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_contract::authorization::{CompiledScope, ContractAccess};
use erp_contract::repository::ContractExt;
use erp_contract::repository::prelude::*;
use erp_core::ids::ContractId;
use erp_permissions::contract;
use erp_support::{FileAssetService, FileAssetView};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::{Error, Result};

#[cfg(test)]
mod tests;

/// 合同可读者的附件读取用例，不要求文件管理菜单资格。
#[derive(Clone)]
pub struct ContractAttachmentReadService {
    db: Database,
    access: ContractAccess,
    files: FileAssetService,
}

impl ContractAttachmentReadService {
    /// 连接合同完整授权和已有文件资产读取步骤。
    /// # 参数
    /// db为业务数据库，access为共享合同授权，files为文件元数据与审计能力。
    /// # 返回
    /// 不缓存账号、附件身份或读取凭证的用例。
    /// # 错误
    /// 构造不执行I/O。
    pub fn new(db: Database, access: ContractAccess, files: FileAssetService) -> Self {
        Self { db, access, files }
    }

    /// 在独立请求的同一快照读取合同附件元数据。
    /// # 参数
    /// contract_id、file_id为精确合同和文件，actor为当前认证账号。
    /// # 返回
    /// 当前合同详情范围及不可变修订关联均通过的文件元数据。
    /// # 错误
    /// 越界、关联不符、文件不存在或交付检查失败时不返回数据。
    pub async fn metadata(
        &self,
        contract_id: &str,
        file_id: &str,
        actor: &AuditActor,
    ) -> Result<FileAssetView> {
        self.read(contract_id, file_id, actor, false).await
    }

    /// 在同一快照读取预览元数据并持久化敏感读取审计。
    /// # 参数
    /// contract_id、file_id为精确合同和文件，actor为当前认证账号。
    /// # 返回
    /// 当前可预览附件的元数据；对象存储读取由调用方在事务外执行。
    /// # 错误
    /// 合同、修订关联、资产、审计或交付检查失败时不返回数据。
    pub async fn preview(
        &self,
        contract_id: &str,
        file_id: &str,
        actor: &AuditActor,
    ) -> Result<FileAssetView> {
        self.read(contract_id, file_id, actor, true).await
    }

    async fn read(
        &self,
        contract_id: &str,
        file_id: &str,
        actor: &AuditActor,
        preview: bool,
    ) -> Result<FileAssetView> {
        let service = self.clone();
        let contract_id = contract_id.to_owned();
        let file_id = file_id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { read(&service, &contract_id, &file_id, &actor, preview, executor).await },
                )
            })
            .await
    }
}

/// 私有测试接缝只表达本用例真实执行步骤，不对业务调用方暴露绕过授权的读取。
#[async_trait]
trait AttachmentRead: Send + Sync {
    type Proof: Send + Sync;
    type Asset: Send;
    async fn query(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<Self::Proof>;
    async fn bound(
        &self,
        proof: &Self::Proof,
        contract_id: &str,
        file_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool>;
    async fn asset(
        &self,
        file_id: &str,
        actor: &AuditActor,
        preview: bool,
        executor: &mut dyn Executor,
    ) -> Result<Self::Asset>;
    async fn validate(&self, proof: &Self::Proof, executor: &mut dyn Executor) -> Result<()>;
}

#[async_trait]
impl AttachmentRead for ContractAttachmentReadService {
    type Proof = CompiledScope;
    type Asset = FileAssetView;

    async fn query(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<Self::Proof> {
        Ok(self.access.resolve(actor, &contract::DETAIL, executor).await?)
    }

    async fn bound(
        &self,
        proof: &CompiledScope,
        contract_id: &str,
        file_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let Some(contract) = self.db.contracts().find_authorized(contract_id, proof, executor).await? else {
            return Ok(false);
        };
        let revisions = self
            .db
            .contract_revisions()
            .list_by_contract(&ContractId::new(contract.base.id), executor)
            .await?;
        Ok(revisions.iter().any(|revision| revision.contract_pdf_file_id.as_ref() == file_id))
    }

    async fn asset(
        &self,
        file_id: &str,
        actor: &AuditActor,
        preview: bool,
        executor: &mut dyn Executor,
    ) -> Result<FileAssetView> {
        if preview {
            Ok(self.files.preview_asset(file_id, actor, executor).await?)
        } else {
            Ok(self.files.asset_view(file_id, executor).await?)
        }
    }

    async fn validate(&self, proof: &CompiledScope, executor: &mut dyn Executor) -> Result<()> {
        Ok(proof.validate(executor).await?)
    }
}

/// 附件只能在原合同查询凭证覆盖的同一阶段完成补齐和交付检查后返回。
async fn read<P: AttachmentRead>(
    port: &P,
    contract_id: &str,
    file_id: &str,
    actor: &AuditActor,
    preview: bool,
    executor: &mut dyn Executor,
) -> Result<P::Asset> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::ValidationError("合同附件读取必须使用快照事务".into()));
    }
    let proof = port.query(actor, executor).await?;
    if !port.bound(&proof, contract_id, file_id, executor).await? {
        return Err(Error::NotFound("合同附件不存在或无权查看".into()));
    }
    let asset = port.asset(file_id, actor, preview, executor).await?;
    port.validate(&proof, executor).await?;
    Ok(asset)
}
