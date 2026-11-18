//! 同次资金查询生成的完整对象存在性计划。
use std::sync::Arc;

use erp_identity::CompiledAccess;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::Executor;

/// 完整金额资格只消费统一服务给出的完整对象计划。
#[derive(Clone, Default)]
pub(crate) struct WholeAccess {
    access: Option<Arc<CompiledAccess>>,
}
impl WholeAccess {
    // 装配资金整单授权时，保存同次统一查询的完整对象计划。
    //
    // 调用参数：
    // access 为原始财务根凭证。
    //
    // 不自行解析政策的完整金额读取载体。
    //
    // 失败边界：
    // 无；缺少完整计划时阶段过滤拒绝全部。
    pub(crate) fn new(access: Arc<CompiledAccess>) -> Self {
        Self { access: Some(access) }
    }
    // 依据根身份应用完整来源计划，不放大记录数量。
    //
    // 将scope_whole设置为完整资格结果的聚合阶段。
    //
    // 失败边界：
    // 无完整计划时固定false。
    pub(crate) fn stages(&self) -> Vec<Document> {
        let Some(access) = &self.access else {
            return membership_stages(Vec::new());
        };
        let Ok(stages) = access.whole_stages() else {
            return membership_stages(Vec::new());
        };

        let alias = "__funds_whole_view";
        let mut pipeline = vec![root_lookup(&access.model().collection, alias, stages.to_vec())];
        pipeline.extend(membership_stages(vec![alias.into()]));
        pipeline
    }
    // 核对资金整单授权边界时，检查共享根凭证的执行边界及有效期。
    //
    // 调用参数：
    // executor 为原读取阶段。
    //
    // 仍有效时成功。
    //
    // 失败边界：
    // 跨事务、跨阶段或到期拒绝。
    pub(crate) fn ensure_current(&self, executor: &mut dyn Executor) -> crate::Result<()> {
        if let Some(access) = &self.access {
            access.ensure_current(executor)?;
        }
        Ok(())
    }
    // 确定资金整单授权的适用条件时，判断中央固定规则是否未授予本次完整根功能资格。
    //
    // 无根凭证或缺少本入口要求的完整功能资格时为true；来源范围仍由阶段过滤。
    pub(crate) fn is_empty(&self) -> bool {
        self.access.as_ref().is_none_or(|access| !access.has_whole_qualification())
    }
    // 处理资金整单授权时，读取完整根计划所属的展示版本。
    //
    // 主授权摘要或固定空版本。
    //
    // 失败边界：
    // 无；版本不能替代授权检查。
    pub(crate) fn version(&self) -> String {
        self.access.as_ref().map_or_else(|| "empty-whole".into(), |access| access.version())
    }
}
const SOURCE_FIELDS: [&str; 3] = ["__funds_sales_ids", "__funds_purchase_ids", "__funds_settlement_ids"];
// 组装资金整单授权的聚合管道时，关联只读取同一个实际根，恢复的预投影变量不能改变根的业务身份。
fn root_lookup(collection: &str, alias: &str, authorized: Vec<Document>) -> Document {
    let mut bindings = doc! { "root_id": "$id", "sources_valid": "$__funds_sources_valid" };
    let mut fields = doc! { "__funds_sources_valid": "$$sources_valid" };
    for (index, field) in SOURCE_FIELDS.iter().enumerate() {
        let variable = format!("source_ids_{index}");
        bindings.insert(&variable, format!("${field}"));
        fields.insert(*field, format!("$${variable}"));
    }

    let mut pipeline =
        vec![doc! { "$match": { "$expr": { "$eq": ["$id", "$$root_id"] } } }, doc! { "$set": fields }];
    pipeline.extend(authorized);
    doc! { "$lookup": { "from": collection, "let": bindings, "pipeline": pipeline, "as": alias } }
}

// 组装资金整单授权的聚合管道时，合并已经通过 identity 的只读视图会员结果，不重新解释来源或人员范围。
fn membership_stages(aliases: Vec<String>) -> Vec<Document> {
    let conditions = aliases
        .iter()
        .map(|alias| Bson::Document(doc! { "$gt": [{ "$size": format!("${alias}") }, 0] }))
        .collect::<Vec<_>>();
    let mut stages = vec![
        doc! { "$set": { "scope_whole": if conditions.is_empty() { Bson::Boolean(false) } else { Bson::Document(doc! { "$or": conditions }) } } },
    ];
    if !aliases.is_empty() {
        stages.push(doc! { "$unset": aliases });
    }
    stages
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_whole_qualification_denies_full_amount() {
        let denied = WholeAccess::default();
        assert!(denied.is_empty());
        assert_eq!(denied.stages(), vec![doc! { "$set": { "scope_whole": false } }]);
    }
}
