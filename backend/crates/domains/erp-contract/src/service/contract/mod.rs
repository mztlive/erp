//! 域 D12 `contract` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 合同首次归档：跨集合（contract + contract_revision + 审计）→
//!   `persistence_core::Transactional::with_transaction`（仓储方法
//!   `create_contract_with_revision` 声明「必须收到事务执行器」）；
//! - 追加版本 / 终止：跨集合（版本 + 主表 CAS + 审计）→ 同一事务模板；
//! - 列表 / 详情：授权与业务读取使用同一事务快照，交付前重新核验。
//!
//! 客户、归属与附件事实走消费方 Port；账号目录使用 identity 公开 API。
//! 文件资产写入与跨域根事务由 `erp-processes` 持有，本模块提供事务内接口。

use std::slice;
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::ids::{CustomerAccountId, FileAssetId};
use erp_identity::AccountDirectoryService;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use crate::authorization::CompiledScope;
pub use crate::authorization::ContractAccess;
use crate::dto::contract::{
    ContractDetailView, ContractRevisionView, ContractView, CreateContractRequest, UploadContractRequest,
};
use crate::entity::contract::{Contract, ContractRevision};
use crate::error::{Error, Result};
use crate::ports::{ContractAuditPort, CustomerAccountFact, CustomerFactsPort, SigningCompanyPort};
use crate::repository::ContractExt;
use crate::repository::prelude::*;

mod archive;
mod commands;
mod company;
mod detail;
mod fingerprint;
mod query;
mod sales_access;
mod scope;

pub use archive::{PlannedContractArchive, plan_first_archive, plan_upload_archive};
pub(crate) use archive::{conflict_if_stale_version, customer_eligibility, plan_next_revision};
use company::{require_signing_company, required_settlement};

/// 合同服务。
///
/// 提供合同归档、版本追加、终止与查询编排。
#[derive(Clone)]
pub struct ContractService {
    db: Database,
    audit: Arc<dyn ContractAuditPort>,
    customers: Arc<dyn CustomerFactsPort>,
    accounts: AccountDirectoryService,
    companies: Arc<dyn SigningCompanyPort>,
    data_scope: ContractAccess,
}

impl ContractService {
    /// 装配归档、终止和查询服务，接入本域数据库、审计及客户/签约主体事实。权限使用组合根注入的固定责任模型；未接线时不放行。
    pub fn new(
        db: Database,
        audit: Arc<dyn ContractAuditPort>,
        customers: Arc<dyn CustomerFactsPort>,
        companies: Arc<dyn SigningCompanyPort>,
        authorization: ContractAccess,
    ) -> Self {
        Self {
            accounts: AccountDirectoryService::new(db.clone()),
            db,
            audit,
            customers,
            companies,
            data_scope: authorization,
        }
    }

    /// 已有 PDF 文件资产时首次归档合同：依请求快照创建稳定身份和首个不可变修订，把 PDF 引用写在该修订上。
    /// 在同一事务核验客户来源资格、客户及签约主体资格，写表头、修订与审计并返回新合同视图。
    /// 非法请求返回 `ValidationError`，客户缺失或停用分别返回 `NotFound` 或业务规则错误；稳定合同编号重复映射 `ConflictError`。
    pub async fn create_contract(
        &self,
        req: CreateContractRequest,
        actor: &AuditActor,
    ) -> Result<ContractView> {
        req.validate()?;
        let customer_id = req.customer_id.to_string();
        let mut planned = plan_first_archive(req, actor.id())?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "contract.create",
            "contract",
            planned.contract.base.id.clone(),
        )?;

        let service = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service.require_create(&actor, &customer_id, executor).await?;

                    service.apply_create(&mut planned.contract, &planned.revision, executor).await?;
                    let view = ContractView::from(planned.contract);
                    service.audit.persist(&audit, executor).await?;
                    Ok(view)
                })
            })
            .await
    }

    /// 上传归档在跨域事务写入前，用调用方分配的文件资产 ID 和必填签约主体规划合同与首修订；只校验输入，不访问数据库、附件或对象存储。
    /// 字段非法时返回错误；客户及主体资格留给原写事务的 `apply_create` 核验。
    pub fn plan_upload(
        &self,
        req: UploadContractRequest,
        file_asset_id: FileAssetId,
        actor_id: &str,
    ) -> Result<PlannedContractArchive> {
        req.validate()?;
        let settlement_party_id = required_settlement(req.settlement_party_id.as_ref())?;
        plan_upload_archive(req, file_asset_id, settlement_party_id, actor_id)
    }

    /// 跨域首次归档已持有根事务时，沿用同一执行器核对当前客户和签约主体资格，再写合同身份及不可变修订，并绑定内存中的当前修订指针。
    /// 不另开事务；唯一索引、乐观锁或数据库写入失败返回对应错误。
    pub async fn apply_create(
        &self,
        contract: &mut Contract,
        revision: &ContractRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.ensure_active_customer(&contract.customer_id, executor).await?;
        require_signing_company(self.companies.as_ref(), &contract.settlement_party_id, executor).await?;

        self.db.contract().create_contract_with_revision(contract, revision, executor).await?;
        Ok(())
    }

    /// 查询合同详情（合同 + 全部不可变版本时间线）。
    /// 返回详情视图（版本按序号倒序）。
    /// `id` 为合同 ID。
    ///
    /// 失败条件：
    /// * `NotFound` - 合同不存在或不在读取范围内
    /// * `ConflictError` - 查询过程中范围变化
    /// 详情经固定授权条件读取，完整补齐后检查原凭证；历史参与只补充读取。
    pub async fn contract_detail(&self, id: &str, actor: &AuditActor) -> Result<ContractDetailView> {
        let service = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { detail::read(&service, &id, &actor, executor).await })
            })
            .await
    }

    // 装载详情展示字段，不解释数据范围。
    // 返回详情视图。
    // `id` 为合同 ID；`scope` 为当前读取阶段的完整合同详情查询凭证。
    // `executor` 为原授权读取事务。
    // 合同不存在时返回 NotFound。
    // 本方法不解释权限；HTTP 详情入口必须先调用 `contract_detail`。
    async fn load_contract_detail(
        &self,
        id: &str,
        scope: &CompiledScope,
        executor: &mut dyn Executor,
    ) -> Result<ContractDetailView> {
        let contract = self
            .db
            .contracts()
            .find_authorized(id, scope, executor)
            .await?
            .ok_or_else(|| Error::NotFound("合同不存在或无权查看".to_string()))?;
        self.detail_view(contract, executor).await
    }

    // 从已授权的原对象或命令结果补齐修订与姓名，不增加详情菜单资格。
    async fn detail_view(
        &self,
        contract: Contract,
        executor: &mut dyn Executor,
    ) -> Result<ContractDetailView> {
        let revisions =
            self.db.contract_revisions().list_by_contract(&contract.base.id.clone().into(), executor).await?;

        let shared_sales_user_ids = contract.shared_sales_user_ids.clone();
        let view: ContractView = contract.into();
        let names = match &view.owner_user_id {
            Some(id) => self.accounts.names(std::slice::from_ref(id), executor).await?,
            None => Default::default(),
        };

        Ok(ContractDetailView {
            shared_sales_user_ids,
            owner_user_name: view.owner_user_id.as_ref().and_then(|id| names.get(id).cloned()),
            owner_user_id: view.owner_user_id,
            id: view.id,
            contract_no: view.contract_no,
            customer_id: view.customer_id,
            settlement_party_id: view.settlement_party_id,
            status: view.status,
            current_revision_id: view.current_revision_id,
            created_at: view.created_at,
            version: view.version,
            revisions: revisions.into_iter().map(ContractRevisionView::from).collect(),
        })
    }

    /// 在调用方事务内证明按指定客户创建合同的资格。
    /// 创建范围覆盖该客户时成功。
    /// `customer_id` 为拟归档合同的客户；沿用调用方执行器 `executor`。
    /// 无创建动作或客户不在授权内时拒绝。
    /// 组合层持有根事务时必须调用本方法，不得只依赖入口事前检查。
    pub async fn require_create(
        &self,
        actor: &AuditActor,
        customer_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.data_scope.require_create(actor, customer_id, executor).await
    }

    /// 校验客户存在且未停用。
    /// 返回客户最小事实。
    /// `customer_id` 为客户角色 ID；`executor` 为原授权写事务的执行器。
    /// 客户不存在或已停用。
    pub async fn ensure_active_customer(
        &self,
        customer_id: &CustomerAccountId,
        executor: &mut dyn Executor,
    ) -> Result<CustomerAccountFact> {
        let facts = self.customers.find_by_ids(slice::from_ref(customer_id), executor).await?;
        customer_eligibility(facts.into_iter().find(|fact| fact.id == customer_id.as_ref()))
    }
}

#[cfg(test)]
mod version_lock_tests {
    use erp_core::ids::{CustomerAccountId, PartyId};

    use super::conflict_if_stale_version;
    use crate::entity::contract::{Contract, ContractData, ContractId};
    use crate::error::Error;

    #[test]
    fn stale_version_returns_conflict_with_stable_retry_message() {
        let contract = Contract::new(
            ContractId::new("c-1"),
            ContractData {
                contract_no: "HT-1".into(),
                customer_id: CustomerAccountId::new("cust-1"),
                settlement_party_id: PartyId::new("party-1"),
            },
            "admin-1",
        )
        .expect("合同必须可构造");
        assert!(conflict_if_stale_version(contract.matches_version(contract.base.version)).is_ok());
        match conflict_if_stale_version(contract.matches_version(0)) {
            Err(Error::ConflictError(message)) => {
                assert_eq!(message, "数据已被其他请求修改，请刷新后重试");
            },
            other => panic!("必须映射为 ConflictError，得到 {other:?}"),
        }
    }

    #[test]
    fn inactive_or_missing_customer_keeps_original_archive_errors() {
        use super::customer_eligibility;
        use crate::ports::CustomerAccountFact;

        match customer_eligibility(None) {
            Err(Error::NotFound(message)) => assert_eq!(message, "客户不存在"),
            other => panic!("期望 NotFound，得到 {other:?}"),
        }
        let disabled = CustomerAccountFact {
            id: "cust-1".to_string(),
            customer_no: "C-1".to_string(),
            party_id: PartyId::new("party-1"),
            is_active: false,
        };
        match customer_eligibility(Some(disabled)) {
            Err(Error::BusinessLogicError(message)) => {
                assert_eq!(message, "客户已停用，禁止归档新合同");
            },
            other => panic!("期望 BusinessLogicError，得到 {other:?}"),
        }
    }
}
