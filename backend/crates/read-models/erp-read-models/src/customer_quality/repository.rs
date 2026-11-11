//! 现任经营质量的销售、客户双重授权查询；交集先于订单上限。

use erp_customer::{CustomerAccess, CustomerExt, CustomerResolvedScope};
use erp_identity::CompiledAccess;
use erp_identity::entity::authorization::QueryOutput;
use erp_sales::entity::sales_order::SalesOrder;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::sales_order::quality::{QUALITY_ORDER_LIMIT, QualityOrderFilter};
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, aggregate};

use crate::{Error, Result};

/// 在同一读取阶段求交销售与客户完整授权，再执行订单数量上限。
/// # 参数
/// db、filter 为数据库及期间筛选，customer、port 为客户授权，executor 为原事务。
/// # 返回
/// 最多 10001 张同时授权的销售单；调用方整体拒绝超限结果。
/// # 错误
/// 凭证缺失、模型不符、授权失效或数据库失败时拒绝。
pub(super) async fn current_orders(
    db: &Database,
    filter: &QualityOrderFilter,
    customer: &CustomerResolvedScope,
    port: &CustomerAccess,
    executor: &mut dyn Executor,
) -> Result<Vec<SalesOrder>> {
    let compiled = customer_access(customer)?;
    port.validate(customer, executor).await?;
    filter.authorized_scope.validate(executor).await?;
    let pipeline = current_pipeline(filter, compiled.stages());
    let collection = db.sales_orders().collection().clone_with_type::<SalesOrder>();
    let orders = aggregate::rows(&collection, pipeline, executor).await?;
    filter.authorized_scope.validate(executor).await?;
    port.validate(customer, executor).await?;
    Ok(orders)
}

fn customer_access(customer: &CustomerResolvedScope) -> Result<&CompiledAccess> {
    let access =
        customer.compiled.as_deref().ok_or_else(|| Error::ValidationError("客户查询缺少授权凭证".into()))?;
    let model = access.model();
    if model.collection != Database::CUSTOMER_ACCOUNTS
        || model.resource != "customer"
        || model.id_field != "id"
        || model.output != QueryOutput::Object
    {
        return Err(Error::ValidationError("客户授权查询模型不匹配".into()));
    }
    Ok(access)
}

fn current_pipeline(filter: &QualityOrderFilter, customer_stages: &[Document]) -> Vec<Document> {
    let mut pipeline = filter.stages();
    let mut customer = vec![doc! {"$match": {"$expr": {"$eq": ["$id", "$$customer_id"]}}}];
    customer.extend_from_slice(customer_stages);
    customer.extend([doc! {"$project": {"_id": 0, "id": 1}}, doc! {"$limit": 1}]);
    pipeline.extend([
        doc! {"$lookup": {"from": Database::CUSTOMER_ACCOUNTS,
        "let": {"customer_id": "$customer_id"}, "pipeline": customer, "as": "_quality_customer"}},
        doc! {"$match": {"_quality_customer.0": {"$exists": true}}},
        doc! {"$unset": "_quality_customer"},
        doc! {"$sort": {"effective_at": 1, "id": 1}},
        doc! {"$limit": i64::try_from(QUALITY_ORDER_LIMIT + 1).expect("订单上限可表示为 i64")},
    ]);
    pipeline
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_sales::repository::sales_order::scope::SalesReadScope;
    use mongodb::bson::Bson;

    use super::*;

    #[test]
    fn customer_authorization_is_joined_by_actual_customer_before_order_limit() {
        let filter = QualityOrderFilter {
            from: 10,
            until: 20,
            customer_ids: Some(vec!["requested".into()]),
            authorized_scope: SalesReadScope::default(),
        };
        let customer_stages = vec![
            doc! {"$lookup": {"from": "customer_assignments", "as": "owners", "pipeline": []}},
            doc! {"$match": {"owners.user_id": "allowed-owner"}},
            doc! {"$unset": "owners"},
        ];
        let pipeline = current_pipeline(&filter, &customer_stages);
        let prefix = filter.stages();
        assert_eq!(&pipeline[..prefix.len()], prefix.as_slice());
        let lookup = pipeline[prefix.len()].get_document("$lookup").unwrap();
        assert_eq!(lookup.get_str("from").unwrap(), Database::CUSTOMER_ACCOUNTS);
        assert_eq!(lookup.get_document("let").unwrap(), &doc! {"customer_id": "$customer_id"});
        let joined = lookup.get_array("pipeline").unwrap();
        assert_eq!(joined[0], Bson::Document(doc! {"$match": {"$expr": {"$eq": ["$id", "$$customer_id"]}}}));
        assert_eq!(&joined[1..4], customer_stages.into_iter().map(Bson::Document).collect::<Vec<_>>());
        assert_eq!(pipeline[prefix.len() + 1], doc! {"$match": {"_quality_customer.0": {"$exists": true}}});
        assert_eq!(pipeline.last().unwrap(), &doc! {"$limit": 10001_i64});
        assert!(pipeline[..pipeline.len() - 1].iter().all(|stage| !stage.contains_key("$limit")));
    }

    #[test]
    fn missing_customer_proof_is_rejected_instead_of_skipping_the_intersection() {
        let context = CustomerResolvedScope {
            user_id: "actor".into(),
            current: true,
            scope_version: String::new(),
            as_of: Instant::from_unix_secs(0),
            compiled: None,
        };
        assert!(
            matches!(customer_access(&context), Err(Error::ValidationError(message)) if message == "客户查询缺少授权凭证")
        );
    }
}
