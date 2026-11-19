//! 单个销售事实翻译；不组合权限规则。

use async_trait::async_trait;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, FactValue, ObjectFacts, QueryModel, QueryModelKey, QueryOutput,
    RelationKey, ResourceTargets,
};
use erp_identity::ports::{AccessFactsPort, AccessMapping, QueryFragment, RelationMapping};
use erp_identity::{Error, Result};
use erp_sales::entity::sales_order::SalesOrder;
use erp_sales::repository::SalesOrderExt;
use erp_workflow::repository::DocumentRegistryExt;
use erp_workflow::repository::document_registry::DocumentParticipantRepositoryExt;
use mongodb::Database;
use mongodb::bson::{Bson, doc};
use persistence_core::Executor;

/// 将销售行映射为固定资源模型，供组合层登记复用。
pub struct SalesMapping {
    model: QueryModel,
}
impl SalesMapping {
    /// 初始化固定销售行模型，供销售授权事实使用。
    ///
    /// # 参数
    /// db 为事实数据库，resource 为服务端固定资源，key 为登记模型标识。
    ///
    /// # 返回
    /// 不执行查询的 Mapping。
    ///
    /// # 错误
    /// 非法模型标识拒绝。
    pub fn new(_db: Database, resource: &str, key: &str) -> Result<Self> {
        Ok(Self {
            model: QueryModel {
                key: QueryModelKey::parse(key)?,
                resource: resource.into(),
                collection: Database::SALES_ORDERS.into(),
                id_field: "id".into(),
                output: QueryOutput::Object,
                facts: [
                    FactKind::Owner,
                    FactKind::Collaborators,
                    FactKind::Organization,
                    FactKind::SettlementParty,
                    FactKind::Warehouse,
                    FactKind::HistoryParticipants,
                ]
                .into(),
            },
        })
    }
}
#[async_trait]
impl AccessMapping for SalesMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn key(&self, _: &AccessContext) -> Result<Bson> {
        Ok("$id".into())
    }
    fn records(&self, _: &AccessContext) -> Result<QueryFragment> {
        Ok(QueryFragment::Match(doc! {"deleted_at":0}))
    }
    fn predicate(&self, fact: &AccessFact, context: &AccessContext) -> Result<QueryFragment> {
        match fact {
            AccessFact::Owned => Ok(QueryFragment::Match(doc! {"sales_owner_user_id":context.actor.id()})),
            AccessFact::Collaborating => Ok(QueryFragment::Match(doc! {"$expr":false})),
            AccessFact::OrganizationIn(ids) => {
                Ok(QueryFragment::Match(doc! {"business_org_unit_id":{"$in":ids.iter().collect::<Vec<_>>()}}))
            },
            AccessFact::SettlementPartyIn(ids) => {
                Ok(QueryFragment::Match(doc! {"settlement_party_id":{"$in":ids.iter().collect::<Vec<_>>()}}))
            },
            AccessFact::WarehouseIn(_) => Ok(QueryFragment::Match(doc! {"$expr":false})),
            AccessFact::HistoryParticipant => Ok(QueryFragment::Exists {
                collection: Database::DOCUMENT_PARTICIPANTS.into(),
                bindings: doc! {"document":"$id"},
                pipeline: vec![
                    doc! {"$match":{"participant_user_id":context.actor.id(),"deleted_at":0,"$expr":{"$eq":["$document_id","$$document"]}}},
                ],
            }),
            _ => Err(Error::ValidationError("销售模型不支持该事实维度".into())),
        }
    }
    fn relation(&self, _: &RelationKey, _: &AccessContext) -> Result<RelationMapping> {
        Err(Error::ValidationError("销售单没有来源关系".into()))
    }
}
pub struct SalesFacts(pub Database);
#[async_trait]
impl AccessFactsPort for SalesFacts {
    async fn load(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>> {
        let targets = ids.ids().iter().cloned().collect::<Vec<_>>();
        let participants =
            self.0.document_participants().document_ids_in(context.actor.id(), &targets, executor).await?;

        let mut result = Vec::new();
        for id in ids.ids() {
            let Some(order) = self.0.sales_orders().find_by_id(id, executor).await? else { continue };
            let child = ObjectFacts {
                resource: "sales_order".into(),
                id: id.clone(),
                version: order.base.version.to_string(),
                owner_id: FactValue::Known(Some(order.sales_owner_user_id.to_string())),
                org_unit_id: FactValue::Known(Some(order.business_org_unit_id.to_string())),
                settlement_party_id: FactValue::Known(Some(order.settlement_party_id.to_string())),
                warehouse_id: FactValue::Known(None),
                collaborators: FactValue::Known(Default::default()),
                history_participants: FactValue::Known(if participants.contains(id) {
                    [context.actor.id().into()].into()
                } else {
                    Default::default()
                }),
                ..Default::default()
            };
            let root = child;
            result.push(root);
        }
        Ok(result)
    }
}

// 组装销售授权事实时，从业务已校验的完整销售单生成候选归属事实。
//
// 调用参数：
// order 为业务实体，不能直接反序列化为授权凭证。
//
// 同对象读取器采用相同归属字段的候选事实。
pub(crate) fn sales_facts(order: &SalesOrder) -> ObjectFacts {
    ObjectFacts {
        resource: "sales_order".into(),
        id: order.base.id.clone(),
        version: order.base.version.to_string(),
        owner_id: FactValue::Known(Some(order.sales_owner_user_id.to_string())),
        org_unit_id: FactValue::Known(Some(order.business_org_unit_id.to_string())),
        settlement_party_id: FactValue::Known(Some(order.settlement_party_id.to_string())),
        warehouse_id: FactValue::Known(None),
        collaborators: FactValue::Known(Default::default()),
        history_participants: FactValue::Known(Default::default()),
        ..Default::default()
    }
}
