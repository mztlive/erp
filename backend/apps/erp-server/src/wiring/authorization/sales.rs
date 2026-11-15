//! 销售事实在质量入口按需展开真实客户关系。
use std::sync::Arc;

use async_trait::async_trait;
use erp_identity::entity::authorization::{AccessContext, ObjectFacts, RelatedObjects, RelationKey};
use erp_identity::{AccessFactsPort, Error, ResourceTargets, Result};
use erp_permissions::internal;
use erp_sales::repository::SalesOrderExt;
use mongodb::Database;
use persistence_core::Executor;

pub(super) struct SalesObjects {
    pub db: Database,
    pub sales: Arc<dyn AccessFactsPort>,
    pub customers: Arc<dyn AccessFactsPort>,
}

#[async_trait]
impl AccessFactsPort for SalesObjects {
    async fn load(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>> {
        let mut objects = self.sales.load(context, ids, executor).await?;
        if context.permission != internal::CUSTOMER_QUALITY_CURRENT {
            return Ok(objects);
        }
        for object in &mut objects {
            let order = self
                .db
                .sales_orders()
                .find_by_id(&object.id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("销售单不存在".into()))?;
            if object.version != order.base.version.to_string() {
                return Err(Error::ConflictError("销售事实读取期间发生变化".into()));
            }
            let id = order.customer_id.to_string();
            let customer_context =
                AccessContext { resource: "customer".into(), model: None, ..context.clone() };
            let targets = ResourceTargets::new(&[&id])?;
            let customers = self.customers.load(&customer_context, &targets, executor).await?;
            object.relations.insert(
                RelationKey::parse("customer")?,
                RelatedObjects { expected_ids: [id].into(), objects: customers },
            );
        }
        Ok(objects)
    }
}

/// 复用销售与客户实现绑定固定销售查询入口。
/// # 参数
/// db、customers、customer_mapping 为共享业务设施。
/// # 返回
/// 根事实、固定查询绑定及供资金来源复用的销售 Mapping。
/// # 错误
/// 查询模型或真实来源描述不匹配时拒绝。
pub(super) fn bindings(
    db: Database,
    customers: Arc<dyn AccessFactsPort>,
    customer_mapping: Arc<dyn erp_identity::AccessMapping>,
) -> Result<(super::FactsBindings, super::MappingBindings, Arc<dyn erp_identity::AccessMapping>)> {
    let sales: Arc<dyn erp_identity::AccessMapping> = Arc::new(
        erp_read_models::sales_center::access::SalesMapping::new(db.clone(), "sales_order", "sales_orders")?,
    );
    let facts: super::FactsBindings = vec![(
        "sales_order".into(),
        Arc::new(SalesObjects {
            db: db.clone(),
            sales: Arc::new(erp_read_models::sales_center::access::SalesFacts(db)),
            customers,
        }),
    )];
    let mut mappings: super::MappingBindings = [
        erp_permissions::sales_order::LIST,
        erp_permissions::sales_order::DETAIL,
        internal::PURCHASE_EXPENSE_SOURCE,
        internal::SALES_ORDER_PROFIT_LOSS_READ,
        internal::SALES_ORDER_EXTRA_COST_READ,
        internal::SALES_CHANGE_DRAFT_READ,
    ]
    .into_iter()
    .map(|p| (p, sales.clone()))
    .collect();
    mappings.push((internal::CUSTOMER_QUALITY_CUSTOMERS, customer_mapping.clone()));
    mappings.push((
        internal::CUSTOMER_QUALITY_CURRENT,
        Arc::new(erp_read_models::customer_quality::QualitySalesMapping::new(
            sales.clone(),
            customer_mapping,
        )?),
    ));
    Ok((facts, mappings, sales))
}
