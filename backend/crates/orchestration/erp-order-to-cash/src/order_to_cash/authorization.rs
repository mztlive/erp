//! 销售命令在原业务事务内重验资源动作、责任和业务版本。

use std::sync::Arc;

use erp_permissions::{Permission as AccessPermission, contract, sales_order as sales_order_permissions};

use crate::services::SalesServices;
mod settlement;

use application_core::AuditActor;
use erp_contract::{Contract, ContractExt};
use erp_customer::{CustomerAccount, CustomerExt};
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::{customer, internal};
use erp_read_models::sales_center::access::SalesAccess;
use erp_sales::entity::sales_order::SalesOrder;
use erp_sales::entity::sales_order::contract_terms::ensure_contract_customer;
use erp_sales::repository::SalesOrderExt;
use persistence_core::Executor;

use super::SalesOrderCommandProcess;
use crate::{Error, Result};

/// 命令上下文只保存请求身份；每次检查均重新解析服务端授权。
#[derive(Clone)]
pub(super) struct SalesCommandAccess {
    pub(super) services: Arc<SalesServices>,
    db: mongodb::Database,
    authorization: Arc<AuthorizationService>,
    access: SalesAccess,
    actor: AuditActor,
    permission: AccessPermission,
    submit: bool,
}

impl SalesOrderCommandProcess {
    /// 构造在原事务执行对象授权的写命令检查器。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 本次销售写动作
    ///
    /// # 返回
    /// 返回可在原事务重验销售单及关联合同／客户的检查器。
    ///
    /// # 错误
    /// 不执行 I/O，不返回错误。
    ///
    /// # 关键业务约束
    /// 关联合同／客户必须另走各自资源动作，不能复用本销售动作码。
    pub(super) fn command_access(
        &self,
        actor: &AuditActor,
        permission: AccessPermission,
    ) -> SalesCommandAccess {
        SalesCommandAccess {
            services: self.services.clone(),
            db: self.db.clone(),
            authorization: self.authorization.clone(),
            access: SalesAccess::new(self.db.clone(), self.authorization.clone()),
            actor: actor.clone(),
            permission,
            submit: false,
        }
    }
}

impl SalesCommandAccess {
    /// 创建并提交必须由同一角色同时提供两个动作。
    ///
    /// # 参数
    /// * `submit` - 为真时追加 `sales_order:submit` 权限要求。
    ///
    /// # 返回
    /// 返回可能追加了提交权限的检查器。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn require_submit(mut self, submit: bool) -> Self {
        self.submit = submit;
        self
    }

    /// 在调用方事务内读取当前可操作单据，并复用公共单对象判定。
    ///
    /// # 参数
    /// * `id` - 销售单主键。
    /// * `executor` - 调用方事务执行器。
    ///
    /// # 返回
    /// 返回当前可操作的销售单。
    ///
    /// # 错误
    /// 单据不可见或缺失时返回错误；已要求提交时，缺少创建权限同样拒绝。
    pub(super) async fn current(&self, id: &str, executor: &mut dyn Executor) -> Result<SalesOrder> {
        let order = if self.permission == sales_order_permissions::CREATE {
            let permission = if self.submit {
                &internal::SALES_CREATE_AND_SUBMIT
            } else {
                &internal::SALES_CREATED_RESULT_READ
            };
            self.authorization.require(&self.actor, permission, AccessTarget::record(id)?, executor).await?;
            self.db
                .sales_orders()
                .find_by_id(id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("销售单不存在或无权操作".into()))?
        } else {
            self.access.require_object(&self.actor, &self.permission, id, executor).await?
        };
        if self.submit {
            self.access.require_object(&self.actor, &sales_order_permissions::SUBMIT, id, executor).await?;
        }
        Ok(order)
    }

    /// 写入前重新读取当前责任与版本，防止预读取后交接或状态变化。
    ///
    /// # 参数
    /// * `id` - 销售单主键。
    /// * `expected` - 预读时的单据版本。
    /// * `executor` - 调用方事务执行器。
    ///
    /// # 返回
    /// 当前版本仍为 `expected` 时返回。
    ///
    /// # 错误
    /// 单据不可操作时返回对应错误；版本已变化时返回冲突。
    pub(super) async fn revalidate(
        &self,
        id: &str,
        expected: u64,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let current = self.current(id, executor).await?;
        if current.base.version != expected {
            return Err(Error::ConflictError("销售单责任或版本已变化，请刷新后重试".into()));
        }
        Ok(())
    }

    /// 新单使用即将持久化的显式责任解释创建范围，不能由创建人审计字段兜底。
    ///
    /// # 参数
    /// * `order` - 即将持久化、带显式责任的销售单。
    /// * `executor` - 调用方事务执行器。
    ///
    /// # 返回
    /// 创建范围允许该责任时返回；已要求提交时，提交范围也必须允许。
    ///
    /// # 错误
    /// 责任不在动作范围内，或权限码无法解析时返回错误。
    pub(super) async fn creation(&self, order: &SalesOrder, executor: &mut dyn Executor) -> Result<()> {
        self.services
            .customer_access
            .require_current(&self.actor, order.customer_id.as_ref(), executor)
            .await?;
        let permission = if self.submit { &internal::SALES_CREATE_AND_SUBMIT } else { &self.permission };
        self.access.require_loaded(&self.actor, permission, order, executor).await?;
        if self.submit {
            self.access
                .require_loaded(&self.actor, &sales_order_permissions::SUBMIT, order, executor)
                .await?;
        }
        Ok(())
    }

    /// 在调用方事务内按合同 detail 动作证明所选合同可见。
    ///
    /// # 参数
    /// * `contract_id` - 本次命令选择的合同
    /// * `executor` - 调用方执行器，必须是原写入事务或同一授权读取
    ///
    /// # 返回
    /// 合同在范围内时成功。
    ///
    /// # 错误
    /// 不可见或缺失时返回 NotFound；缺动作时拒绝。
    ///
    /// # 关键业务约束
    /// handler 事前检查不能代替；不得用无范围 `find_by_id` 作为授权读取。
    async fn require_contract(&self, contract_id: &str, executor: &mut dyn Executor) -> Result<()> {
        self.services
            .contracts
            .require_with(self.actor.clone(), &contract::DETAIL, contract_id, executor)
            .await?;
        Ok(())
    }

    /// 在调用方事务内按客户 detail 动作证明所选客户可见。
    ///
    /// # 参数
    /// * `customer_id` - 合同所属客户
    /// * `executor` - 调用方执行器，必须是原写入事务或同一授权读取
    ///
    /// # 返回
    /// 客户在范围内时成功。
    ///
    /// # 错误
    /// 不可见或缺失时返回 NotFound；缺动作时拒绝。
    ///
    /// # 关键业务约束
    /// 不得用销售范围或合同范围代替客户对象判定。
    async fn require_customer(&self, customer_id: &str, executor: &mut dyn Executor) -> Result<()> {
        self.services
            .customer_access
            .require_with(self.actor.clone(), &customer::DETAIL, customer_id, executor)
            .await?;
        Ok(())
    }

    /// 在调用方事务内分别重验所选合同与客户。
    ///
    /// # 参数
    /// * `contract_id` - 本次命令选择的合同
    /// * `customer_id` - 合同所属客户
    /// * `executor` - 原写入事务执行器
    ///
    /// # 返回
    /// 两个对象均在范围内时成功。
    ///
    /// # 错误
    /// 任一对象不可见时返回 NotFound。
    ///
    /// # 关键业务约束
    /// 创建／保存／提交必须在原写入事务调用；独立 HTTP 事前检查不是凭证。
    pub(super) async fn related(
        &self,
        contract_id: &str,
        customer_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.require_contract(contract_id, executor).await?;
        self.require_customer(customer_id, executor).await?;
        Ok(())
    }

    /// 按销售单已绑定合同与客户在调用方事务内重验跨域 detail 范围。
    ///
    /// # 参数
    /// * `order` - 即将写入的销售单，带明确客户及可选合同
    /// * `executor` - 原写入事务执行器
    ///
    /// # 返回
    /// 合同与客户均在范围内时成功。
    ///
    /// # 错误
    /// 客户或已选合同越权时返回 NotFound。
    ///
    /// # 关键业务约束
    /// 客户始终独立重验；只有已选择合同时才重验合同对象。
    pub(super) async fn related_order(&self, order: &SalesOrder, executor: &mut dyn Executor) -> Result<()> {
        if let Some(contract_id) = order.contract_id.as_ref() {
            let contract = self.load_contract(contract_id.as_ref(), executor).await?;
            ensure_contract_customer(&contract.customer_id, Some(&order.customer_id))?;
            self.require_customer(order.customer_id.as_ref(), executor).await?;
            self.settlement_name(&order.settlement_party_id, executor).await?;
            Ok(())
        } else {
            self.load_customer(order.customer_id.as_ref(), executor).await?;
            self.settlement_name(&order.settlement_party_id, executor).await?;
            Ok(())
        }
    }

    /// 在调用方执行器内按合同 detail 动作装载合同实体。
    ///
    /// # 参数
    /// * `contract_id` - 所选合同稳定身份
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 合同在范围内时返回实体。
    ///
    /// # 错误
    /// 不可见或缺失时返回 NotFound。
    ///
    /// # 关键业务约束
    /// `find_by_id` 只在 `require_with` 之后装载业务字段，不得单独作为授权。
    pub(super) async fn load_contract(
        &self,
        contract_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Contract> {
        self.require_contract(contract_id, executor).await?;
        self.db
            .contracts()
            .find_by_id(contract_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("合同不存在或无权查看".into()))
    }

    /// 在调用方执行器内按客户 detail 动作装载客户实体。
    ///
    /// # 参数
    /// * `customer_id` - 合同所属客户
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 客户在范围内时返回实体。
    ///
    /// # 错误
    /// 不可见或缺失时返回 NotFound。
    ///
    /// # 关键业务约束
    /// `find_by_id` 只在 `require_with` 之后装载业务字段，不得单独作为授权。
    pub(super) async fn load_customer(
        &self,
        customer_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<CustomerAccount> {
        self.require_customer(customer_id, executor).await?;
        self.db
            .customer_accounts()
            .find_by_id(customer_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("客户不存在或无权查看".into()))
    }
}
