//! 跨域只读检查：检查人须能读取目标单据，目标账号不产生会话或业务写入。
use application_core::AuditActor;
use erp_identity::SharedRbacService;
use erp_identity::dto::inspection::{AccessInspectionRequest, AccessInspectionView};
use erp_identity::service::access_control::inspection::AccessInspectionService;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::sales_center::access::SalesAccess;
use crate::{Error, Result};

/// 编排真实身份解析和已接线业务对象判定。
#[derive(Clone)]
pub struct AccessInspectionReadService {
    db: Database,
    rbac: SharedRbacService,
}

impl AccessInspectionReadService {
    /// 绑定现有领域服务。
    /// # 参数
    /// * `db` - 数据库。
    /// * `rbac` - RBAC 服务。
    /// # 返回
    /// 只读检查器。
    /// # 错误
    /// 无。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 在同一读取事务内检查配置及可选具体单据。
    /// # 参数
    /// * `operator` - 发起检查者。
    /// * `request` - 人员、业务、操作及可选单据。
    /// # 返回
    /// 分层检查结果，不返回业务单据内容。
    /// # 错误
    /// 管理越权、检查人不可读单据及存储故障返回错误。
    pub async fn inspect(
        &self,
        operator: AuditActor,
        request: AccessInspectionRequest,
    ) -> Result<AccessInspectionView> {
        let this = self.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let inspected = AccessInspectionService::new(this.db.clone(), this.rbac.clone())
                        .inspect(&operator, &request, executor)
                        .await?;
                    let mut view = inspected.view;
                    if let Some(target) = inspected.actor {
                        if let Some(id) = &request.object_id {
                            this.sales_object(&operator, &target, &request.action, id, &mut view, executor)
                                .await?;
                        } else {
                            view.push(
                                "业务条件",
                                "review",
                                "本次仅检查配置；单据责任、历史参与、状态及命令内容仍须在实际操作时重验。",
                            );
                        }
                    }
                    Ok(view)
                })
            })
            .await
    }

    /// 使用销售真实对象资格及实体编辑状态规则，先检查检查人的读取权。
    async fn sales_object(
        &self,
        operator: &AuditActor,
        target: &AuditActor,
        action: &str,
        id: &str,
        view: &mut AccessInspectionView,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let service = SalesAccess::new(self.db.clone(), self.rbac.clone());
        service.require_object(operator, "detail", id, &[], executor).await?;
        match service.require_object(target, action, id, &[], executor).await {
            Ok(order) => {
                view.push("具体单据范围", "passed", "该人员满足此销售单的操作权限与数据范围条件。");
                if action == "update" {
                    if order.allows_first_submission_working_copy() {
                        view.push(
                            "单据状态",
                            "passed",
                            "当前单据允许编辑草稿。保存时仍需校验关联合同、客户、商品及提交内容。",
                        );
                        view.push("保存内容", "review", "本检查不提交修改内容，不能代替完整保存校验。");
                    } else {
                        view.push(
                            "单据状态",
                            "blocked",
                            "当前销售单不是可编辑草稿或已经结案，不能保存工作副本。",
                        );
                    }
                }
            },
            Err(Error::NotFound(_) | Error::Forbidden(_)) => {
                view.push(
                    "具体单据范围",
                    "blocked",
                    "该人员不能执行此操作。请核对角色范围、个人限制、当前负责人和有效历史参与资格。",
                );
            },
            Err(error) => return Err(error),
        }
        Ok(())
    }
}
