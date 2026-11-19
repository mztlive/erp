//! 启动描述与实际 Mapping 的连接；运行时不得替换模型或来源实现。
use std::collections::BTreeMap;
use std::sync::Arc;

use erp_core::common::time::Instant;
use mongodb::bson::{Bson, Document};

use crate::entity::authorization::{
    AccessContext, AccessFact, AccessRule, MappingFacts, QueryModel, QueryOutput, RelationKey, RelationMatch,
};
use crate::ports::{AccessMapping, MappingPreparation, QueryFragment, RelationMapping};
use crate::{Error, Result};

type MappingCache = Vec<(Arc<dyn AccessMapping>, Arc<dyn AccessMapping>)>;

/// 启动时冻结模型、固定阶段与来源绑定的映射包装，运行时仍委托原映射取证。
pub(crate) struct FrozenMapping {
    // 实际提供主键、事实条件和动态准备的原映射实例。
    inner: Arc<dyn AccessMapping>,
    // 启动时校验并冻结的模型描述。
    model: QueryModel,
    // 启动时校验通过的只读固定准备阶段。
    stages: Vec<Document>,
    // 固定阶段的 BSON 字节，用于检测包括字段顺序在内的变化。
    stage_bytes: Vec<Vec<u8>>,
    // 启动时登记的原来源实例，供运行时核对实例身份。
    originals: BTreeMap<RelationKey, Arc<dyn AccessMapping>>,
    // 对应关系键下供编译器使用的冻结来源实例。
    relations: BTreeMap<RelationKey, Arc<dyn AccessMapping>>,
}

impl FrozenMapping {
    /// 启动时递归校验并冻结模型与固定阶段；按原实例复用包装，只允许完整对象来源，超出登记深度或数量限制时拒绝。
    pub(crate) fn freeze(
        inner: Arc<dyn AccessMapping>,
        cache: &mut MappingCache,
        depth: usize,
    ) -> Result<Arc<dyn AccessMapping>> {
        if depth > 16 || cache.len() > 1000 {
            return Err(Error::ValidationError("Mapping 来源过深或过多".into()));
        }
        if let Some((_, frozen)) = cache.iter().find(|(original, _)| Arc::ptr_eq(original, &inner)) {
            return Ok(frozen.clone());
        }

        let model = inner.model().clone();
        model.validate()?;
        let stages = inner.stages();
        validate_stages(&stages)?;
        let stage_bytes = encode_stages(&stages)?;

        let originals = inner.relations();
        let mut relations = BTreeMap::new();
        for (key, mapping) in &originals {
            if mapping.model().output != QueryOutput::Object {
                return Err(Error::ValidationError("来源 Mapping 必须提供完整对象".into()));
            }
            relations.insert(key.clone(), Self::freeze(mapping.clone(), cache, depth + 1)?);
        }

        let frozen: Arc<dyn AccessMapping> =
            Arc::new(Self { inner: inner.clone(), model, stages, stage_bytes, originals, relations });
        cache.push((inner, frozen.clone()));
        Ok(frozen)
    }

    // 运行时对照冻结描述和原始 BSON 字节，拒绝模型或固定阶段变化。
    fn ensure_model(&self) -> Result<()> {
        if self.inner.model() != &self.model || encode_stages(&self.inner.stages())? != self.stage_bytes {
            return Err(Error::ValidationError("Mapping 描述在构造后改变".into()));
        }
        Ok(())
    }
}

impl AccessMapping for FrozenMapping {
    // 对编译器暴露启动时校验的模型，不依赖原映射当前描述。
    fn model(&self) -> &QueryModel {
        &self.model
    }

    // 提供冻结后的来源登记表，使编译器沿固定关系解析。
    fn relations(&self) -> BTreeMap<RelationKey, Arc<dyn AccessMapping>> {
        self.relations.clone()
    }

    // 返回冻结时的只读固定阶段，不使用原映射的当前版本。
    fn stages(&self) -> Vec<Document> {
        self.stages.clone()
    }

    // 沿当前授权上下文向原映射取得静态有效期边界。
    fn valid_until(&self, context: &AccessContext) -> Option<Instant> {
        self.inner.valid_until(context)
    }

    // 沿用原映射声明的可选动态准备能力。
    fn preparation(&self) -> Option<&dyn MappingPreparation> {
        self.inner.preparation()
    }

    // 核对冻结描述未变后，委托原映射解析授权记录主键。
    fn key(&self, context: &AccessContext, prepared: Option<&MappingFacts>) -> Result<Bson> {
        self.ensure_model()?;
        self.inner.key(context, prepared)
    }

    // 核对冻结描述后，委托原映射生成实际未删除记录的条件。
    fn records(&self, context: &AccessContext, prepared: Option<&MappingFacts>) -> Result<QueryFragment> {
        self.ensure_model()?;
        self.inner.records(context, prepared)
    }

    // 核对冻结描述后，传入原上下文和已准备事实转换当前条件。
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        prepared: Option<&MappingFacts>,
    ) -> Result<QueryFragment> {
        self.ensure_model()?;
        self.inner.predicate(fact, context, prepared)
    }

    // 核对冻结描述和运行时来源实例，保留原引用表达式，改用启动时冻结的来源包装。
    fn relation(
        &self,
        relation: &RelationKey,
        context: &AccessContext,
        prepared: Option<&MappingFacts>,
    ) -> Result<RelationMapping> {
        self.ensure_model()?;
        let actual = self.inner.relation(relation, context, prepared)?;
        let original = self
            .originals
            .get(relation)
            .ok_or_else(|| Error::ValidationError("Mapping 来源未在启动描述中登记".into()))?;
        if !Arc::ptr_eq(original, &actual.mapping) {
            return Err(Error::ValidationError("Mapping 来源在构造后改变".into()));
        }
        Ok(RelationMapping { expected_ids: actual.expected_ids, mapping: self.relations[relation].clone() })
    }
}

// 将固定阶段逐项编码为 BSON 字节，以便比较时保留字段先后顺序。
fn encode_stages(stages: &[Document]) -> Result<Vec<Vec<u8>>> {
    stages
        .iter()
        .map(|stage| stage.to_vec().map_err(|_| Error::ValidationError("Mapping 固定阶段无法编码".into())))
        .collect()
}

// 启动时只允许 `$lookup`、`$set`、`$addFields` 固定准备，不得筛选、展开或分页根记录，也不得写库。
fn validate_stages(stages: &[Document]) -> Result<()> {
    for stage in stages {
        if stage.len() != 1
            || !stage.keys().all(|key| matches!(key.as_str(), "$lookup" | "$set" | "$addFields"))
        {
            return Err(Error::ValidationError("Mapping 固定准备不得改变根记录集合".into()));
        }
        reject_writes(&Bson::Document(stage.clone()))?;
    }
    Ok(())
}

// 递归检查文档和数组，拒绝固定准备阶段及嵌套管道中的 `$out`、`$merge`。
fn reject_writes(value: &Bson) -> Result<()> {
    match value {
        Bson::Document(document) => {
            for (key, value) in document {
                if matches!(key.as_str(), "$out" | "$merge") {
                    return Err(Error::ValidationError("Mapping 固定准备必须只读".into()));
                }
                reject_writes(value)?;
            }
        },
        Bson::Array(values) => {
            for value in values {
                reject_writes(value)?;
            }
        },
        _ => {},
    }
    Ok(())
}

/// 装配时递归核对固定规则所需事实、来源模型和记录授权资源；超深、维度缺失、资源错配或任一来源规则试图输出完整对象时拒绝。
pub(crate) fn validate_rule(rule: &AccessRule, mapping: &dyn AccessMapping, depth: usize) -> Result<()> {
    if depth > 16 {
        return Err(Error::ValidationError("授权来源嵌套超限".into()));
    }
    match rule {
        AccessRule::Fact(fact) if !mapping.model().facts.contains(&fact.kind()) => {
            return Err(Error::ValidationError(format!(
                "Mapping {}（{}）缺少固定事实 {:?}",
                mapping.model().key.as_str(),
                mapping.model().resource,
                fact.kind()
            )));
        },
        AccessRule::Required { facts, rule } => {
            if !facts.is_subset(&mapping.model().facts) {
                return Err(Error::ValidationError(format!(
                    "Mapping {}（{}）缺少必需事实 {:?}",
                    mapping.model().key.as_str(),
                    mapping.model().resource,
                    facts.difference(&mapping.model().facts).collect::<Vec<_>>()
                )));
            }
            validate_rule(rule, mapping, depth + 1)?;
        },
        AccessRule::All(rules) | AccessRule::Any(rules) => {
            for rule in rules {
                validate_rule(rule, mapping, depth + 1)?;
            }
        },
        AccessRule::Independent { rule, .. } => validate_rule(rule, mapping, depth + 1)?,
        AccessRule::RelationPresence { relation, .. } => {
            require_relation(mapping, relation)?;
        },
        AccessRule::Related { relation, resource, mode, rule } => {
            let child = require_relation(mapping, relation)?;
            if child.model().resource != *resource || child.model().output != QueryOutput::Object {
                return Err(Error::ValidationError("来源 Mapping 资源或输出错配".into()));
            }
            if *mode == RelationMatch::Any && mapping.model().output == QueryOutput::Object {
                return Err(Error::ValidationError("任一来源不能输出完整对象".into()));
            }
            validate_rule(rule, child.as_ref(), depth + 1)?;
        },
        AccessRule::RecordReadGrant(resource) if mapping.model().resource != resource.code() => {
            return Err(Error::ValidationError("记录授权与 Mapping 资源错配".into()));
        },
        _ => {},
    }
    Ok(())
}

// 按关系键取得启动时登记的来源映射，缺失时报告接入错误。
fn require_relation(mapping: &dyn AccessMapping, key: &RelationKey) -> Result<Arc<dyn AccessMapping>> {
    mapping
        .relations()
        .remove(key)
        .ok_or_else(|| Error::ValidationError(format!("缺少固定来源 Mapping：{}", key.as_str())))
}
