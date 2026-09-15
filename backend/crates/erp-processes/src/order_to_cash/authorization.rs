//! 销售命令在原业务事务内重验资源动作、责任和业务版本。

use application_core::AuditActor;
use erp_contract::{Contract, ContractExt};
use erp_customer::{CustomerAccount, CustomerExt};
use erp_identity::{Permission, SharedRbacService};
use erp_read_models::sales_center::access::SalesAccess;
use erp_sales::entity::sales_order::SalesOrder;
use persistence_core::Executor;

use super::SalesOrderCommandProcess;
use crate::adapters::{contract_access, customer_access};
use crate::{Error, Result};

/// 命令上下文只保存请求身份；每次检查均重新解析服务端授权。
#[derive(Clone)]
pub(super) struct SalesCommandAccess {
    db: mongodb::Database,
    rbac: SharedRbacService,
    access: SalesAccess,
    actor: AuditActor,
    action: &'static str,
    permissions: Vec<Permission>,
}

impl SalesOrderCommandProcess {
    /// 构造写命令检查器；缺少身份装配时拒绝，不退回路由级授权。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 本次销售写动作
    ///
    /// # 返回
    /// 返回可在原事务重验销售单及关联合同／客户的检查器。
    ///
    /// # 错误
    /// 未注入 RBAC 时拒绝。
    ///
    /// # 关键业务约束
    /// 关联合同／客户必须另走各自资源动作，不能复用本销售动作码。
    pub(super) fn command_access(
        &self,
        actor: &AuditActor,
        action: &'static str,
    ) -> Result<SalesCommandAccess> {
        let rbac = self.require_rbac()?.clone();
        Ok(SalesCommandAccess {
            db: self.db.clone(),
            rbac: rbac.clone(),
            access: SalesAccess::new(self.db.clone(), rbac),
            actor: actor.clone(),
            action,
            permissions: vec![],
        })
    }
}

impl SalesCommandAccess {
    /// 创建并提交必须由同一角色同时提供两个动作。
    pub(super) fn require_submit(mut self, submit: bool) -> Result<Self> {
        if submit {
            self.permissions.push(Permission::parse("sales_order:submit")?);
        }
        Ok(self)
    }

    /// 在调用方事务内读取当前可操作单据，并复用公共单对象判定。
    pub(super) async fn current(&self, id: &str, executor: &mut dyn Executor) -> Result<SalesOrder> {
        let order = self
            .access
            .require_object(&self.actor, self.action, id, &self.permissions, executor)
            .await?;
        if !self.permissions.is_empty() {
            self.access
                .require_object(
                    &self.actor,
                    "submit",
                    id,
                    &[Permission::parse("sales_order:create")?],
                    executor,
                )
                .await?;
        }
        Ok(order)
    }

    /// 写入前重新读取当前责任与版本，防止预读取后交接或状态变化。
    pub(super) async fn revalidate(
        &self,
        id: &str,
        expected: u64,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let current = self.current(id, executor).await?;
        if current.base.version != expected {
            return Err(Error::ConflictError(
                "销售单责任或版本已变化，请刷新后重试".into(),
            ));
        }
        Ok(())
    }

    /// 新单使用即将持久化的显式责任解释创建范围，不能由创建人审计字段兜底。
    pub(super) async fn creation(&self, order: &SalesOrder, executor: &mut dyn Executor) -> Result<()> {
        self.require_creation_action(order, self.action, &self.permissions, executor)
            .await?;
        if !self.permissions.is_empty() {
            self.require_creation_action(
                order,
                "submit",
                &[Permission::parse("sales_order:create")?],
                executor,
            )
            .await?;
        }
        Ok(())
    }

    /// 创建与提交分别使用当前动作范围，禁止把读取参与权用于写入。
    async fn require_creation_action(
        &self,
        order: &SalesOrder,
        action: &str,
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let (access, scope) = self
            .access
            .resolve(&self.actor, action, permissions, executor)
            .await?;
        if !SalesAccess::allows(&access, &scope, order)? {
            return Err(Error::Forbidden("没有该业务责任范围的销售建单权限".into()));
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
        contract_access(self.db.clone(), self.rbac.clone())
            .require_with(self.actor.clone(), "detail", contract_id, executor)
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
        customer_access(self.db.clone(), self.rbac.clone())
            .require_with(self.actor.clone(), "detail", customer_id, executor)
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
    /// * `order` - 即将写入的销售单，必须已带合同与客户
    /// * `executor` - 原写入事务执行器
    ///
    /// # 返回
    /// 合同与客户均在范围内时成功。
    ///
    /// # 错误
    /// 缺少合同绑定返回校验错误；越权对象返回 NotFound。
    ///
    /// # 关键业务约束
    /// 无合同的来源不得走本销售写命令绑定路径。
    pub(super) async fn related_order(&self, order: &SalesOrder, executor: &mut dyn Executor) -> Result<()> {
        let Some(contract_id) = order.contract_id.as_ref() else {
            return Err(Error::ValidationError(
                "销售单缺少关联合同，无法重验合同范围".into(),
            ));
        };
        self.related(contract_id.as_ref(), order.customer_id.as_ref(), executor)
            .await
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
