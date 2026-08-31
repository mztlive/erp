//! 随业务命令提交的文件资产事务载荷。
//!
//! 对象字节由 HTTP 层在事务外写入对象存储；本模块只构造文件资产实体、解析
//! 请求内临时引用，并让业务服务把文件元数据与业务聚合写入同一 MongoDB 事务。

use std::collections::HashSet;

use database::{AccessControlExt, FileAssetExt};
use entities::{
    file_asset::{FileAsset, PendingFileReference, PendingFileReferenceSet, SensitivityClass},
    ids::FileAssetId,
    AuditLog,
};
use id_generator::next_id;
use mongodb::{ClientSession, Database};
use validator::Validate;

use crate::{audit::AuditActor, errors::Result, file_asset::PendingFileAssetRequest};

/// 已完成实体构造、等待随业务聚合一起持久化的文件资产集合。
pub(crate) struct PendingFileAssets {
    assets: Vec<FileAsset>,
    audits: Vec<AuditLog>,
    references: PendingFileReferenceSet,
}

impl PendingFileAssets {
    /// 校验临时引用和登记元数据，并生成正式文件资产身份及审计事件。
    pub(crate) fn prepare(requests: Vec<PendingFileAssetRequest>, actor: &AuditActor) -> Result<Self> {
        let mut assets = Vec::with_capacity(requests.len());
        let mut audits = Vec::with_capacity(requests.len());
        let mut references = Vec::with_capacity(requests.len());
        for request in requests {
            let reference = PendingFileReference::parse(&request.reference)?;
            request.registration.validate()?;
            let sensitivity = request.registration.sensitivity_class;
            let asset = FileAsset::new(
                FileAssetId::new(next_id()),
                request.registration.into_data(actor.id())?,
            )?;
            let asset_id = FileAssetId::new(asset.base.id.clone());
            let audit =
                actor
                    .clone()
                    .resource_log("file_asset.register", "file_asset", asset.base.id.clone())?;
            references.push((reference, asset_id, sensitivity));
            assets.push(asset);
            audits.push(audit);
        }
        Ok(Self {
            assets,
            audits,
            references: PendingFileReferenceSet::new(references)?,
        })
    }

    /// 把业务 DTO 中的临时文件引用替换为本批次生成的正式资产 ID。
    ///
    /// 返回 `true` 表示发生了替换；普通既有资产 ID 保持不变。
    pub(crate) fn resolve_id(&self, id: &mut FileAssetId, used: &mut HashSet<String>) -> Result<bool> {
        Ok(self.references.resolve_id(id, used)?)
    }

    /// 校验每个已上传文件都被业务 DTO 引用，禁止在成功事务中产生孤儿元数据。
    pub(crate) fn ensure_all_used(&self, used: &HashSet<String>) -> Result<()> {
        Ok(self.references.ensure_all_used(used)?)
    }

    /// 判断正式资产 ID 是否属于本次待登记批次。
    pub(crate) fn contains_id(&self, id: &FileAssetId) -> bool {
        self.references.contains_id(id)
    }

    /// 返回本次待登记资产的敏感级别。
    pub(crate) fn sensitivity(&self, id: &FileAssetId) -> Option<SensitivityClass> {
        self.references.sensitivity(id)
    }

    /// 在调用方已经建立的事务内登记全部文件资产及各自审计事件。
    pub(crate) async fn persist(&self, db: &Database, session: &mut ClientSession) -> Result<()> {
        db.file_assets()
            .create_many_ordered(&self.assets, session)
            .await?;
        db.audit_logs().create_many_ordered(&self.audits, session).await?;
        Ok(())
    }
}
