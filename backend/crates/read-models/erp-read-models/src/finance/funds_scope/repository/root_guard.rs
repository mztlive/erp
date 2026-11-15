//! 资金根授权在数据库水合及分页前统一执行。
use erp_identity::CompiledAccess;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, aggregate};

use super::super::{FundsAccess, FundsAuthorization};
use crate::Result;

impl FundsAccess {
    /// 在水合单个资金根前执行同一凭证的完整根过滤。
    /// # 参数
    /// authorization 为当前根凭证，id 为真实根身份，executor 为原读取阶段。
    /// # 返回
    /// 根存在且固定来源及完整性要求满足时成功。
    /// # 错误
    /// 根缺失或无权均返回 NotFound，数据库与凭证错误原样传播。
    pub(in crate::finance::funds_scope) async fn require_visible_root(
        &self,
        authorization: &FundsAuthorization,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let access = authorization.root_access()?;
        access.ensure_current(executor)?;
        let mut pipeline = vec![doc! { "$match": { "id": id } }];
        pipeline.extend(root_membership_stages(access));
        pipeline.push(doc! { "$project": { "id": 1 } });
        let rows: Vec<Document> =
            aggregate::rows(&self.db.collection(&access.model().collection), pipeline, executor).await?;
        if rows.is_empty() {
            return Err(crate::Error::NotFound("资金单据不存在或无权查看".into()));
        }
        Ok(())
    }
}

impl FundsAuthorization {
    /// 保留根身份且在分页前应用完整授权与真实来源完整性。
    /// # 参数
    /// 无。
    /// # 返回
    /// 只读根关联过滤；不展开根记录或交付额外字段。
    /// # 错误
    /// 无凭证时产生拒绝全部的过滤。
    pub(in crate::finance::funds_scope) fn guard_stages(&self) -> Vec<Document> {
        let Some(access) = &self.access else {
            return vec![doc! { "$match": { "$expr": false } }];
        };
        let mut authorized = vec![doc! { "$match": { "$expr": { "$eq": ["$id", "$$root"] } } }];
        authorized.extend(root_membership_stages(access));
        authorized.push(doc! { "$project": { "_id": 0, "id": 1 } });
        vec![
            doc! { "$lookup": { "from": &access.model().collection, "let": { "root": "$id" }, "pipeline": authorized, "as": "__funds_root_allowed" } },
            doc! { "$match": { "__funds_root_allowed.0": { "$exists": true } } },
            doc! { "$unset": "__funds_root_allowed" },
        ]
    }
}

/// 根会员同时接受匹配份额和完整整账路径，允许有整账职责的真实零分配根。
fn root_membership_stages(access: &CompiledAccess) -> Vec<Document> {
    let Ok(whole) = access.whole_stages() else {
        return access.stages().to_vec();
    };
    let identity = doc! {"$project": {"_id":0,"id":1}};
    let mut partial = access.stages().to_vec();
    partial.push(identity.clone());
    let mut whole = whole.to_vec();
    whole.push(identity);
    vec![
        doc! {"$facet": {"partial":partial,"whole":whole}},
        doc! {"$match": {"$or": [
            {"partial.0":{"$exists":true}}, {"whole.0":{"$exists":true}}
        ]}},
    ]
}
