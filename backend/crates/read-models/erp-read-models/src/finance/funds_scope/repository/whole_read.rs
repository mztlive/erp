//! 对当前已读取的完整实际引用执行统一完整单据视图。

use mongodb::bson::{Bson, Document, doc};
use persistence_core::Executor;

use super::super::{FundsAccess, FundsAuthorization};
use crate::{Error, Result};

impl FundsAccess {
    /// 在原事务查询当前根是否属于统一完整单据视图。
    /// # 参数
    /// authorization 为当前读取凭证，id 为根身份，sales/purchases 为仓储实际读取的完整引用。
    /// # 返回
    /// identity 的完整视图包含该根时为 true。
    /// # 错误
    /// 未知根模型、授权失效或数据库读取失败保留对应错误。
    pub(in crate::finance::funds_scope) async fn whole_allowed(
        &self,
        authorization: &FundsAuthorization,
        id: &str,
        sales: &[String],
        purchases: &[String],
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let collection = match authorization.context.resource.as_str() {
            "customer_receipt" => "customer_receipts",
            "supplier_payment" => "supplier_payments",
            "invoice" => "invoices",
            _ => return Err(Error::ValidationError("未知完整资金模型".into())),
        };
        authorization.validate(executor).await?;

        let mut pipeline = vec![
            doc! { "$match": { "id": id, "deleted_at": 0 } },
            super::whole_facts::fields(
                Bson::Array(sales.iter().cloned().map(Bson::String).collect()),
                Bson::Array(purchases.iter().cloned().map(Bson::String).collect()),
                true.into(),
            ),
        ];
        pipeline.extend(authorization.whole.stages());
        pipeline.extend([doc! { "$match": { "scope_whole": true } }, doc! { "$project": { "id": 1 } }]);
        let rows: Vec<Document> =
            super::aggregate(self.db.collection(collection), pipeline, executor).await?;

        authorization.validate(executor).await?;
        Ok(!rows.is_empty())
    }
}
