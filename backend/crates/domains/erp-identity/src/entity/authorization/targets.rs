use std::collections::BTreeSet;

use erp_core::validation::normalize_required_text_ref;

use super::ObjectFacts;
use crate::{Error, Result};

/// 本次需要检查的目标集合；不得用它表示账号的完整授权集合。
#[derive(Debug, Clone)]
pub struct ResourceTargets(BTreeSet<String>);
impl ResourceTargets {
    // 对象端口返回后按本次请求核对完整根身份集合，缺失、重复或额外对象均拒绝，不按部分结果判断权限。
    pub(crate) fn ensure_objects(&self, resource: &str, objects: &[ObjectFacts]) -> Result<()> {
        let found = objects.iter().map(|object| object.id.clone()).collect::<BTreeSet<_>>();
        if self.0.is_empty()
            || found != self.0
            || objects.len() != found.len()
            || objects.iter().any(|object| object.resource != resource)
        {
            return Err(Error::NotFound("对象不存在或必要目标不完整".into()));
        }
        Ok(())
    }
    /// 校验目标身份并去重；数量只受本次操作规模约束。
    /// 排序后的唯一目标，空集不查询授权集合。
    ///
    /// 超限、空白、超长或未规范化身份拒绝。
    ///
    /// ids 为服务端确定的完整操作目标，最多 1000 项。
    pub fn new(ids: &[&str]) -> Result<Self> {
        if ids.len() > 1000 {
            return Err(Error::ValidationError("单次最多检查 1000 个目标".into()));
        }
        let mut targets = BTreeSet::new();
        for id in ids {
            if normalize_required_text_ref(id, "目标不能为空", 128, "目标过长")? != *id {
                return Err(Error::ValidationError("目标身份不能带首尾空白".into()));
            }
            targets.insert((*id).to_owned());
        }
        Ok(Self(targets))
    }
    /// 取得已校验的目标集合。
    /// 不可变、去重后的目标 ID。
    pub fn ids(&self) -> &BTreeSet<String> {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_are_bounded_normalized_and_deduplicated() {
        assert_eq!(
            ResourceTargets::new(&["b", "a", "a"]).unwrap().ids(),
            &BTreeSet::from(["a".into(), "b".into()])
        );
        assert!(ResourceTargets::new(&[" a"]).is_err());
        assert!(ResourceTargets::new(&[""]).is_err());
        assert!(ResourceTargets::new(&vec!["a"; 1001]).is_err());
        assert!(ResourceTargets::new(&[]).unwrap().ids().is_empty());
    }

    #[test]
    fn complete_roots_reject_missing_extra_duplicate_and_wrong_resource() {
        let targets = ResourceTargets::new(&["a", "b"]).unwrap();
        let a = ObjectFacts { resource: "order".into(), id: "a".into(), ..Default::default() };
        let b = ObjectFacts { id: "b".into(), ..a.clone() };
        assert!(targets.ensure_objects("order", &[a.clone(), b.clone()]).is_ok());
        for objects in [vec![a.clone()], vec![a.clone(), a.clone()], vec![a.clone(), b.clone(), b.clone()]] {
            assert!(targets.ensure_objects("order", &objects).is_err());
        }
        assert!(targets.ensure_objects("customer", &[a, b]).is_err());
        assert!(ResourceTargets::new(&[]).unwrap().ensure_objects("order", &[]).is_err());
    }
}
