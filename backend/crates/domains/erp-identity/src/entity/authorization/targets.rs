use std::collections::BTreeSet;

use erp_core::validation::normalize_required_text_ref;

use super::ObjectFacts;
use crate::{Error, Result};

/// 当前操作待核验的记录身份；账号的记录授权范围须另行取证。
#[derive(Debug, Clone)]
pub struct ResourceTargets(
    // 经输入校验、去重并按 ID 排序的目标集合。
    BTreeSet<String>
);
impl ResourceTargets {
    /// 取证后核对非空目标集；缺失、重复、额外或资源错配的根对象均使整次检查失败。
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
    /// 校验并去重服务端确认的完整目标，去重前最多接收 1000 项。
    /// 每个 ID 必须非空、最多 128 个字符且无首尾空白；空集可构造，记录授权查询会跳过空集。
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
    /// 借用去重后的目标 ID，供本次事实读取和身份核对使用。
    pub fn ids(&self) -> &BTreeSet<String> {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 验证输入规模和 ID 格式限制，以及去重后保留空集合的行为。
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

    // 验证根对象必须与非空目标集完全对应，资源类型也须一致。
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
