//! 供应商资料修订事务载荷与落库。
//!
//! 与准备步骤分文件存放，修订语义保持不变。

use std::collections::HashMap;
use std::sync::Arc;

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::ids::{SupplierCapabilityId, SupplierQualificationId};
use erp_party::{
    Party, PartyAddress, PartyBankAccount, PartyContact, PartyExt, PartyRevision, PartyTaxProfile,
};
use erp_supplier::{
    SupplierAccount, SupplierCapability, SupplierCapabilityRevision, SupplierCommercialProfileRevision,
    SupplierExt, SupplierProfileCommand, SupplierProfileCommandData, SupplierProfileMutationView,
    SupplierQualification, SupplierQualificationCapability, SupplierQualificationRevision,
    SupplierRatingRevision, command_view,
};
use erp_support::PendingAttachmentBatch;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;

/// 主体从属事实的追加式变更。
#[derive(Default)]
pub(super) struct PartyFactChanges {
    pub(super) contacts: Vec<PartyContact>,
    pub(super) new_contact: Option<PartyContact>,
    pub(super) addresses: Vec<PartyAddress>,
    pub(super) new_address: Option<PartyAddress>,
    pub(super) tax_profiles: Vec<PartyTaxProfile>,
    pub(super) new_tax_profile: Option<PartyTaxProfile>,
    pub(super) bank_accounts: Vec<PartyBankAccount>,
    pub(super) new_bank_account: Option<PartyBankAccount>,
}

/// 能力当前集合变更及其不可变快照。
#[derive(Default)]
pub(super) struct CapabilityChanges {
    pub(super) ids: HashMap<String, SupplierCapabilityId>,
    pub(super) created: Vec<SupplierCapability>,
    pub(super) updated: Vec<SupplierCapability>,
    pub(super) revisions: Vec<SupplierCapabilityRevision>,
}

/// 资质当前集合变更、不可变快照与能力关联替换。
#[derive(Default)]
pub(super) struct QualificationChanges {
    pub(super) created: Vec<SupplierQualification>,
    pub(super) updated: Vec<SupplierQualification>,
    pub(super) revisions: Vec<SupplierQualificationRevision>,
    pub(super) replacements: Vec<(SupplierQualificationId, Vec<SupplierQualificationCapability>)>,
}

/// 评级开放区间关闭与下一版本。
#[derive(Default)]
pub(super) struct RatingChanges {
    pub(super) current: Option<SupplierRatingRevision>,
    pub(super) created: Option<SupplierRatingRevision>,
}

/// 供应商资料修订的主体与变更集合。
///
/// # 用途
/// 将 Party/供应商根与事实差异打包，供 [`PreparedUpdate::new`] 构造事务载荷。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 主体与差异必须已完成实体构造，本结构不再二次校验。
pub(super) struct PreparedUpdateContext {
    /// 已更新的 Party 根。
    pub(super) party: Party,
    /// 新 Party 修订。
    pub(super) party_revision: PartyRevision,
    /// 已更新的供应商根。
    pub(super) supplier: SupplierAccount,
    /// 新商业资料修订。
    pub(super) commercial_profile: SupplierCommercialProfileRevision,
    /// 从属事实差异。
    pub(super) facts: PartyFactChanges,
    /// 能力差异。
    pub(super) capabilities: CapabilityChanges,
    /// 资质差异。
    pub(super) qualifications: QualificationChanges,
    /// 评级差异。
    pub(super) ratings: RatingChanges,
}

/// 已校验并完成实体构造的修订事务载荷。
pub(super) struct PreparedUpdate {
    pub(super) party: Party,
    pub(super) party_revision: PartyRevision,
    pub(super) supplier: SupplierAccount,
    pub(super) commercial_profile: SupplierCommercialProfileRevision,
    pub(super) facts: PartyFactChanges,
    pub(super) capabilities: CapabilityChanges,
    pub(super) qualifications: QualificationChanges,
    pub(super) ratings: RatingChanges,
    pub(super) command: SupplierProfileCommand,
    pub(super) audit: erp_audit::AuditLog,
    pub(super) result: SupplierProfileMutationView,
    pub(super) pending_assets: Arc<dyn PendingAttachmentBatch>,
}

impl PreparedUpdate {
    /// 构造修订结果、幂等记录与根审计。
    ///
    /// # 用途
    /// 由已构造主体与变更集合生成幂等命令、审计与稳定结果。
    ///
    /// # 参数
    /// * `context` - Party/供应商根与变更集合
    /// * `idempotency_key` - 客户端幂等键
    /// * `request_fingerprint` - 请求摘要
    /// * `effective_from` - 生效起始日
    /// * `change_reason` - 变更原因
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回可落库的修订事务载荷。
    ///
    /// # 错误
    /// 命令字段非法时返回错误。
    ///
    /// # 关键业务约束
    /// 供应商版本按当前根版本加一写入命令。
    pub(super) fn new(
        context: PreparedUpdateContext,
        idempotency_key: String,
        request_fingerprint: String,
        effective_from: erp_core::common::time::BusinessDate,
        change_reason: String,
        actor: &AuditActor,
        pending_assets: Arc<dyn PendingAttachmentBatch>,
    ) -> Result<Self> {
        let PreparedUpdateContext {
            party,
            party_revision,
            supplier,
            commercial_profile,
            facts,
            capabilities,
            qualifications,
            ratings,
        } = context;
        let command = SupplierProfileCommand::new(
            next_id(),
            SupplierProfileCommandData {
                idempotency_key,
                operation: "update".to_string(),
                request_fingerprint,
                supplier_id: supplier.base.id.clone(),
                supplier_no: supplier.supplier_no.clone(),
                revision_id: commercial_profile.base.id.clone(),
                revision_no: commercial_profile.revision.revision_no,
                supplier_version: supplier.base.version + 1,
                effective_from,
                change_reason,
            },
        )?;
        let result = command_view(command.clone());
        let audit = actor.clone().resource_log(
            "supplier_profile.update",
            "supplier_profile",
            result.supplier_id.clone(),
        )?;
        Ok(Self {
            party,
            party_revision,
            supplier,
            commercial_profile,
            facts,
            capabilities,
            qualifications,
            ratings,
            command,
            audit,
            result,
            pending_assets,
        })
    }

    /// 将完整资料修订与幂等结果写入同一事务。
    pub(super) async fn persist(mut self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        self.pending_assets.persist(db, executor).await?;
        self.persist_roots(db, executor).await?;
        self.facts.persist(db, executor).await?;
        self.capabilities.persist(db, executor).await?;
        self.qualifications.persist(db, executor).await?;
        self.ratings.persist(db, executor).await?;
        db.supplier_profile_commands().create(&self.command, executor).await?;
        db.audit_logs().create(&self.audit, executor).await?;
        Ok(())
    }

    /// 写入 Party/Supplier 根及新修订。
    async fn persist_roots(&mut self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        db.party_revisions().create(&self.party_revision, executor).await?;
        db.parties().update(&mut self.party, executor).await?;
        db.supplier_commercial_profile_revisions().create(&self.commercial_profile, executor).await?;
        db.supplier_accounts().update(&mut self.supplier, executor).await?;
        Ok(())
    }
}

impl PartyFactChanges {
    /// 写入从属事实的停用与新事实行。
    async fn persist(mut self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        for item in &mut self.contacts {
            db.party_contacts().update(item, executor).await?;
        }
        if let Some(item) = &self.new_contact {
            db.party_contacts().create(item, executor).await?;
        }
        for item in &mut self.addresses {
            db.party_addresses().update(item, executor).await?;
        }
        if let Some(item) = &self.new_address {
            db.party_addresses().create(item, executor).await?;
        }
        for item in &mut self.tax_profiles {
            db.party_tax_profiles().update(item, executor).await?;
        }
        if let Some(item) = &self.new_tax_profile {
            db.party_tax_profiles().create(item, executor).await?;
        }
        for item in &mut self.bank_accounts {
            db.party_bank_accounts().update(item, executor).await?;
        }
        if let Some(item) = &self.new_bank_account {
            db.party_bank_accounts().create(item, executor).await?;
        }
        Ok(())
    }
}

impl CapabilityChanges {
    /// 写入能力快照及当前实体变更。
    async fn persist(mut self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        for revision in &self.revisions {
            db.supplier_capability_revisions().create(revision, executor).await?;
        }
        for capability in &self.created {
            db.supplier_capabilities().create(capability, executor).await?;
        }
        for capability in &mut self.updated {
            db.supplier_capabilities().update(capability, executor).await?;
        }
        Ok(())
    }
}

impl QualificationChanges {
    /// 写入资质快照、当前实体和整体替换后的能力关联。
    async fn persist(mut self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        for revision in &self.revisions {
            db.supplier_qualification_revisions().create(revision, executor).await?;
        }
        for qualification in &self.created {
            db.supplier_qualifications().create(qualification, executor).await?;
        }
        for qualification in &mut self.updated {
            db.supplier_qualifications().update(qualification, executor).await?;
        }
        for (qualification_id, links) in self.replacements {
            db.supplier().replace_qualification_capabilities(&qualification_id, links, executor).await?;
        }
        Ok(())
    }
}

impl RatingChanges {
    /// 关闭上一开放区间并写入下一评级版本。
    async fn persist(mut self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        if let Some(current) = self.current.as_mut() {
            db.supplier_rating_revisions().update(current, executor).await?;
        }
        if let Some(created) = &self.created {
            db.supplier_rating_revisions().create(created, executor).await?;
        }
        Ok(())
    }
}
