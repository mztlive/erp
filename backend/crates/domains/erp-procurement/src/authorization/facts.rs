use async_trait::async_trait;
use erp_identity::entity::authorization::{
    AccessContext, FactValue, ObjectFacts, RelatedObjects, RelationKey,
};
use erp_identity::{AccessFactsPort, ResourceTargets, Result};
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::purchase_order::PurchaseOrder;
use crate::repository::PurchaseOrderExt;

/// 通过采购拥有域仓储读取完整根目标及当前本单事实。
pub struct PurchaseFacts {
    db: Database,
}

impl PurchaseFacts {
    /// 绑定采购拥有域数据库。
    /// # 参数
    /// db 为与业务事务相同的数据库。
    /// # 返回
    /// 不保存授权结果的事实提供器。
    /// # 错误
    /// 构造不执行 I/O。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 将原执行器读取的真实采购单转换为统一授权对象事实。
    /// # 参数
    /// order 为当前采购事实，resource 为服务端固定的根或来源资源。
    /// # 返回
    /// 真实身份、版本、当前责任人、业务组织及目标仓库；不补造缺失责任人。
    /// # 错误
    /// 不执行 I/O；缺少责任人的事实保持已知空值。
    pub fn object(order: &PurchaseOrder, resource: &str) -> ObjectFacts {
        ObjectFacts {
            resource: resource.into(),
            id: order.base.id.clone(),
            version: order.base.version.to_string(),
            owner_id: FactValue::Known(order.owner_user_id.clone().filter(|id| !id.trim().is_empty())),
            org_unit_id: FactValue::Known(Some(order.business_org_unit_id.clone())),
            warehouse_id: FactValue::Known(order.target_warehouse_id.as_ref().map(ToString::to_string)),
            ..Default::default()
        }
    }
}

#[async_trait]
impl AccessFactsPort for PurchaseFacts {
    async fn load(
        &self,
        context: &AccessContext,
        targets: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>> {
        let mut result = Vec::new();
        for id in targets.ids() {
            let Some(order) = self.db.purchase_orders().find_by_id(id, executor).await? else { continue };
            let mut root = Self::object(&order, &context.resource);
            root.relations.insert(
                RelationKey::parse("self_purchase")?,
                RelatedObjects {
                    expected_ids: [order.base.id.clone()].into(),
                    objects: vec![Self::object(&order, "purchase_order")],
                },
            );
            result.push(root);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{
        PurchaseOrderId, SalesOrderId, SalesOrderRevisionId, SupplierAccountId, WarehouseId,
    };

    use super::*;
    use crate::entity::facts::PaymentTermFact;
    use crate::entity::purchase_order::{FulfillmentResponsibility, PurchaseOrderData, PurchaseType};

    fn order() -> PurchaseOrder {
        PurchaseOrder::new(
            PurchaseOrderId::new("purchase"),
            PurchaseOrderData {
                business_org_unit_id: "procurement-org".into(),
                purchase_no: "PO-1".into(),
                sales_order_id: SalesOrderId::new("sales"),
                sales_order_revision_id: SalesOrderRevisionId::new("revision"),
                creation_basis_id: "basis".into(),
                supplier_id: SupplierAccountId::new("supplier"),
                purchase_type: PurchaseType::Physical,
                payment_term_code: "CASH".into(),
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                owner_user_id: "buyer".into(),
                target_warehouse_id: Some(WarehouseId::new("warehouse")),
            },
            "creator",
            |code| Ok(PaymentTermFact::new(code)),
        )
        .unwrap()
    }

    #[test]
    fn current_owner_org_warehouse_are_known_and_creator_does_not_fill_missing_owner() {
        let mut order = order();
        let facts = PurchaseFacts::object(&order, "purchase_order");
        assert_eq!(facts.owner_id, FactValue::Known(Some("buyer".into())));
        assert_eq!(facts.org_unit_id, FactValue::Known(Some("procurement-org".into())));
        assert_eq!(facts.warehouse_id, FactValue::Known(Some("warehouse".into())));
        assert_eq!(facts.version, order.base.version.to_string());
        for missing in [None, Some("   \t".into())] {
            order.owner_user_id = missing;
            assert_eq!(PurchaseFacts::object(&order, "purchase_order").owner_id, FactValue::Known(None));
        }
    }
}
