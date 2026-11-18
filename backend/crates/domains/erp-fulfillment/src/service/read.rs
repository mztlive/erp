//! 履约管理读取入口；命令和审批步骤沿用本域原执行器渲染事实。
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::{Permission, fulfillment};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::FulfillmentService;
use crate::Result;
use crate::dto::{
    DeliveryDetailView, DeliveryListParams, DeliveryView, ElectronicDeliveryListParams,
    ElectronicDeliveryView, PageView, PurchaseReceiptDetailView, PurchaseReceiptListParams,
    PurchaseReceiptView, ServiceFulfillmentListParams, ServiceFulfillmentView,
};

/// 面向认证账号的本域管理读取服务；共享授权实例由应用注入。
pub struct FulfillmentReadService {
    domain: FulfillmentService,
    authorization: Arc<AuthorizationService>,
}
impl FulfillmentReadService {
    /// 装配履约管理读取服务，使用本域数据库和启动时注入的共享授权服务；不执行 I/O，也不缓存请求身份或权限。
    pub fn new(db: Database, authorization: Arc<AuthorizationService>) -> Self {
        Self { domain: FulfillmentService::new(db), authorization }
    }

    // 在一个快照事务中执行固定功能授权及完整履约读取。
    async fn read<T: Send + 'static, F>(
        &self,
        actor: &AuditActor,
        permission: Permission,
        load: F,
    ) -> Result<T>
    where
        F: for<'a> FnOnce(&'a FulfillmentService, &'a mut dyn Executor) -> ReadOutput<'a, T> + Send + 'static,
    {
        let domain = FulfillmentService::new(self.domain.db.clone());
        let authorization = self.authorization.clone();
        let actor = actor.clone();
        self.domain
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorized_read(&domain, &authorization, &actor, &permission, executor, load).await
                })
            })
            .await
    }

    /// 按筛选条件读取采购收货列表；在同一个快照中先检查当前账号的固定列表权限，再装配分页视图。
    /// 身份或权限不满足、输入非法或读取失败时不交付结果。
    pub async fn purchase_receipt_list(
        &self,
        params: &PurchaseReceiptListParams,
        actor: &AuditActor,
    ) -> Result<PageView<PurchaseReceiptView>> {
        let params = params.clone();
        self.read(actor, fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_LIST, move |domain, executor| {
            Box::pin(async move { domain.load_purchase_receipts(&params, executor).await })
        })
        .await
    }

    /// 按真实单据 ID 读取采购收货详情（表头和收货行）；固定详情权限检查与数据读取共用一个快照。
    /// 身份或权限不满足、记录缺失或读取失败时不交付结果。
    pub async fn purchase_receipt_detail(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<PurchaseReceiptDetailView> {
        let id = id.to_owned();
        self.read(actor, fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_DETAIL, move |domain, executor| {
            Box::pin(async move { domain.load_purchase_receipt_view(&id, executor).await })
        })
        .await
    }

    /// 按筛选条件读取实物发货列表；在同一个快照中先检查当前账号的固定列表权限，再装配分页视图。
    /// 身份或权限不满足、输入非法或读取失败时不交付结果。
    pub async fn delivery_list(
        &self,
        params: &DeliveryListParams,
        actor: &AuditActor,
    ) -> Result<PageView<DeliveryView>> {
        let params = params.clone();
        self.read(actor, fulfillment::DELIVERY_DELIVERY_LIST, move |domain, executor| {
            Box::pin(async move { domain.load_deliveries(&params, executor).await })
        })
        .await
    }

    /// 按真实单据 ID 读取实物发货详情（表头和发货行）；固定详情权限检查与数据读取共用一个快照。
    /// 身份或权限不满足、记录缺失或读取失败时不交付结果。
    pub async fn delivery_detail(&self, id: &str, actor: &AuditActor) -> Result<DeliveryDetailView> {
        let id = id.to_owned();
        self.read(actor, fulfillment::DELIVERY_DELIVERY_DETAIL, move |domain, executor| {
            Box::pin(async move { domain.load_delivery_view(&id, executor).await })
        })
        .await
    }

    /// 按筛选条件读取电子交付列表；在同一个快照中先检查当前账号的固定列表权限，再装配分页视图。
    /// 身份或权限不满足、输入非法或读取失败时不交付结果。
    pub async fn electronic_delivery_list(
        &self,
        params: &ElectronicDeliveryListParams,
        actor: &AuditActor,
    ) -> Result<PageView<ElectronicDeliveryView>> {
        let params = params.clone();
        self.read(
            actor,
            fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_LIST,
            move |domain, executor| {
                Box::pin(async move { domain.load_electronic_deliveries(&params, executor).await })
            },
        )
        .await
    }

    /// 按真实单据 ID 读取电子交付详情（交付记录）；固定详情权限检查与数据读取共用一个快照。
    /// 身份或权限不满足、记录缺失或读取失败时不交付结果。
    pub async fn electronic_delivery_detail(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<ElectronicDeliveryView> {
        let id = id.to_owned();
        self.read(
            actor,
            fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_DETAIL,
            move |domain, executor| {
                Box::pin(async move { domain.load_electronic_delivery_view(&id, executor).await })
            },
        )
        .await
    }

    /// 按筛选条件读取服务履约列表；在同一个快照中先检查当前账号的固定列表权限，再装配分页视图。
    /// 身份或权限不满足、输入非法或读取失败时不交付结果。
    pub async fn service_fulfillment_list(
        &self,
        params: &ServiceFulfillmentListParams,
        actor: &AuditActor,
    ) -> Result<PageView<ServiceFulfillmentView>> {
        let params = params.clone();
        self.read(
            actor,
            fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_LIST,
            move |domain, executor| {
                Box::pin(async move { domain.load_service_fulfillments(&params, executor).await })
            },
        )
        .await
    }

    /// 按真实单据 ID 读取服务履约详情（履约记录）；固定详情权限检查与数据读取共用一个快照。
    /// 身份或权限不满足、记录缺失或读取失败时不交付结果。
    pub async fn service_fulfillment_detail(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<ServiceFulfillmentView> {
        let id = id.to_owned();
        self.read(
            actor,
            fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_DETAIL,
            move |domain, executor| {
                Box::pin(async move { domain.load_service_fulfillment_view(&id, executor).await })
            },
        )
        .await
    }
}

type ReadOutput<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

// 使用原执行器先完成固定资格检查，再执行本次完整读取。
async fn authorized_read<T: Send + 'static, F>(
    domain: &FulfillmentService,
    authorization: &AuthorizationService,
    actor: &AuditActor,
    permission: &Permission,
    executor: &mut dyn Executor,
    load: F,
) -> Result<T>
where
    F: for<'a> FnOnce(&'a FulfillmentService, &'a mut dyn Executor) -> ReadOutput<'a, T> + Send,
{
    authorization.require(actor, permission, AccessTarget::Operation, executor).await?;
    load(domain, executor).await
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::Error;

    // 验证八个功能入口共用的真实读取编排在授权拒绝后不执行任何业务读取。
    #[tokio::test]
    async fn all_reads_stop_before_business_io_when_authorization_is_unavailable() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("unit_test");
        let domain = FulfillmentService::new(db);
        let authorization = unavailable_authorization();
        let actor = AuditActor::new("actor".into(), "operator".into(), AccountKind::Admin);

        for permission in [
            fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_LIST,
            fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_DETAIL,
            fulfillment::DELIVERY_DELIVERY_LIST,
            fulfillment::DELIVERY_DELIVERY_DETAIL,
            fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_LIST,
            fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_DETAIL,
            fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_LIST,
            fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_DETAIL,
        ] {
            let result: Result<()> = authorized_read(
                &domain,
                &authorization,
                &actor,
                &permission,
                &mut SnapshotExecutor::default(),
                |_, _| Box::pin(async { panic!("未授权不得读取业务数据") }),
            )
            .await;
            assert!(matches!(result, Err(Error::Internal(reason)) if reason == "测试授权设施不可用"));
        }
    }
}
