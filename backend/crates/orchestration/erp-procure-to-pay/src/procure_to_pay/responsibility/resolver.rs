//! 采购责任解析所需的目录事实加载、规则选择与负责人资格校验。

use std::slice;

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_permissions::purchase_order;
use erp_procurement::dto::procurement_responsibility::ProcurementResponsibilityResolutionView;
use erp_procurement::entity::procurement_responsibility::{
    ProcurementResponsibilityResolutionIdentity, ProcurementResponsibilityResolutionLine,
};
use erp_procurement::service::procurement_responsibility::{
    CandidateResolution, ProcurementResponsibilityService,
};
use persistence_core::{Executor, NoTransaction};

use super::ProcurementResponsibilityProcess;
use super::adapter::ResponsibilityFactsAdapter;
use crate::{Error, Result};

/// 内部批量解析输入；实体负责行键与区域规范化。
pub type ResolutionInput = ProcurementResponsibilityResolutionLine;

/// 已授权的批量解析行：领域身份与展示姓名分离持有.
///
/// # 参数
/// * `identity` - 忽略展示姓名的稳定责任身份
/// * `owner_name` - 负责人当前展示姓名，仅用于最后一跳视图映射
///
/// # 返回
/// 返回可生成视图但身份不受姓名影响的授权行.
///
/// # 错误
/// 无.
///
/// # 约束
/// 姓名变化不得影响计划身份；行顺序、负责人、规则 ID 或规则类型任一变化必须拒绝.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedResolutionLine {
    /// 忽略展示姓名的稳定责任身份.
    pub identity: ProcurementResponsibilityResolutionIdentity,
    /// 负责人当前展示姓名.
    pub owner_name: String,
}

/// 已授权的批量解析计划.
///
/// # 参数
/// * `lines` - 按输入顺序排列的授权行，直接持有领域身份
/// * `policy_revision` - 校验负责人权限使用的策略版本
///
/// # 返回
/// 返回逐行具体负责人身份及授权策略版本.
///
/// # 错误
/// 无.
///
/// # 约束
/// 计划直接持有 `ProcurementResponsibilityResolutionIdentity`；展示姓名和 API View 只在最后一跳映射.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedResolutionPlan {
    /// 按输入顺序排列的逐行授权结果.
    pub lines: Vec<AuthorizedResolutionLine>,
    /// 校验负责人权限使用的策略版本.
    pub policy_revision: u64,
}

impl AuthorizedResolutionPlan {
    // 将授权计划映射为 API 预览视图.
    //
    // 无，消费计划行的身份与展示姓名。返回与输入顺序一致的解析视图；这是展示姓名进入 View 的唯一位置.无；纯内存映射.
    //
    // 仅在 Service 最后一跳调用；重验比较必须使用 `identities` 而非本方法结果.
    pub(crate) fn views(&self) -> Vec<ProcurementResponsibilityResolutionView> {
        self.lines
            .iter()
            .map(|line| ProcurementResponsibilityResolutionView {
                line_key: line.identity.line_key.clone(),
                owner_user_id: line.identity.owner_user_id.clone(),
                owner_name: line.owner_name.clone(),
                rule_id: line.identity.rule_id.clone(),
                rule_type: line.identity.rule_type,
            })
            .collect()
    }

    // 返回保持输入顺序的稳定责任身份集合.返回忽略展示姓名的稳定责任身份集合.无；纯内存克隆.
    //
    // 姓名变化不得影响结果；顺序、负责人、规则 ID 或规则类型任一变化必须体现在返回值差异中.
    pub(crate) fn identities(&self) -> Vec<ProcurementResponsibilityResolutionIdentity> {
        self.lines.iter().map(|line| line.identity.clone()).collect()
    }
}

impl ProcurementResponsibilityProcess {
    /// 批量解析并校验每位负责人拥有采购单创建权限。
    ///
    /// `inputs` 为稳定行键、SKU 与服务区域。返回逐行具体负责人及授权策略版本。目录事实缺失、规则零/多命中、负责人不可登录或无采购建单权限时失败关闭。
    pub async fn resolve_strict(&self, inputs: &[ResolutionInput]) -> Result<AuthorizedResolutionPlan> {
        self.strict_plan(inputs, &mut NoTransaction).await
    }

    /// 使用调用方事务解析目录、规则与具体负责人的当前资格。
    ///
    /// `inputs` 为已规范化的稳定销售行与SKU身份；`executor` 为原业务命令执行器。返回同一快照取得的责任计划和授权策略版本。
    ///
    /// 目录、规则、人员或采购建单资格不满足时拒绝。
    pub async fn strict_plan(
        &self,
        inputs: &[ResolutionInput],
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedResolutionPlan> {
        let candidates = self.resolve_candidates(inputs, executor).await?;
        self.authorize_candidates(candidates, executor).await
    }

    /// 销售形式化实际派采购任务前，在原业务事务复验事务外按原稳定行取得的责任计划，并占用整单默认轮次。
    /// 目录、规则、负责人账号或授权策略版本变化时失败，由销售生效事务回滚。
    pub async fn claim_plan(
        &self,
        inputs: &[ResolutionInput],
        expected: &AuthorizedResolutionPlan,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let candidates = self.resolve_candidates(inputs, executor).await?;
        super::authorization::revalidate_candidates(&self.db, &self.rbac, &candidates, expected, executor)
            .await?;

        ProcurementResponsibilityService::new(self.db.clone()).claim_pools(&candidates, executor).await?;
        Ok(())
    }

    /// 稳定校验指定账号可作为具体采购负责人。
    ///
    /// `owner_user_id` 为负责人账号 ID。账号可登录、为后台管理员且拥有 `purchase_order:create` 时返回稳定策略版本。
    ///
    /// 账号失效、缺少权限或身份设施读取失败时返回对应错误。
    pub async fn authorize_owner_eligibility(&self, owner_user_id: &str) -> Result<u64> {
        let permission = purchase_order::CREATE;
        let actor = AuditActor::new(owner_user_id.into(), owner_user_id.into(), AccountKind::Admin);
        let qualified = self
            .rbac
            .permissions(&actor, slice::from_ref(&permission), &mut NoTransaction)
            .await?
            .ok_or_else(|| {
                Error::ValidationError(format!("采购负责人 {owner_user_id} 必须为可登录后台账号"))
            })?;
        if !qualified.same_role(&[permission]) {
            return Err(Error::ValidationError(format!(
                "采购负责人 {owner_user_id} 无权创建采购单，请调整负责人或联系管理员配置权限"
            )));
        }
        Ok(qualified.policy_revision())
    }

    // 在调用方事务中按采购规则解析候选；不授予采购任务或普通单据访问权。
    //
    // `inputs` 为稳定需求行和 SKU；`executor` 为调用方事务。按输入顺序返回通过账号资格校验的候选。调用方必须另验其业务执行权限。
    //
    // 目录、规则、账号缺失或存在冲突时失败关闭。
    pub(crate) async fn resolve_candidates(
        &self,
        inputs: &[ResolutionInput],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CandidateResolution>> {
        Ok(ProcurementResponsibilityService::new(self.db.clone())
            .resolve_candidates(inputs, &ResponsibilityFactsAdapter { db: self.db.clone() }, executor)
            .await?)
    }

    /// 询价任务分派前，沿原事务为整单需求优先匹配询价池候选，不占用轮次。
    /// 结果还须经过询价业务权限校验；目录、规则池或人员失效时拒绝。
    pub async fn resolve_inquiry_candidates(
        &self,
        inputs: &[ResolutionInput],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CandidateResolution>> {
        Ok(ProcurementResponsibilityService::new(self.db.clone())
            .resolve_inquiry_candidates(inputs, &ResponsibilityFactsAdapter { db: self.db.clone() }, executor)
            .await?)
    }

    // 对候选负责人执行 RBAC 校验并冻结原执行器内的策略版本。
    //
    // `candidates` 为账号状态已通过的候选责任；`executor` 为候选解析使用的原执行器。返回授权计划和稳定策略版本。
    //
    // 任一负责人缺少采购建单权限或策略无法稳定读取时返回错误。
    pub(super) async fn authorize_candidates(
        &self,
        candidates: Vec<CandidateResolution>,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedResolutionPlan> {
        super::authorization::authorize_candidates(&self.db, &self.rbac, candidates, executor).await
    }
}

// 候选责任的目录与账号资格校验通过后，用稳定的需求行、负责人及规则生成领域责任身份，不经 API 视图转换。
// 展示姓名只随授权行携带；行键、负责人或规则 ID 非法时返回实体错误。
pub(super) fn authorized_line(candidate: CandidateResolution) -> Result<AuthorizedResolutionLine> {
    let identity = ProcurementResponsibilityResolutionIdentity::new(
        candidate.line_key,
        candidate.owner_user_id,
        candidate.rule_id,
        candidate.rule_type,
    )
    .map_err(Error::Logic)?;
    Ok(AuthorizedResolutionLine { identity, owner_name: candidate.owner_name })
}

// 将候选责任集合转换为稳定责任身份集合.
//
// `candidates` 为已完成目录与账号资格校验的候选责任。返回保持输入顺序的稳定责任身份集合.任一候选行键、负责人或规则 ID 非法时返回实体错误.
//
// 经由领域构造函数直接生成，不经过 API View；姓名变化不得影响结果.
pub(super) fn plan_identities(
    candidates: &[CandidateResolution],
) -> Result<Vec<ProcurementResponsibilityResolutionIdentity>> {
    candidates
        .iter()
        .map(|candidate| {
            ProcurementResponsibilityResolutionIdentity::new(
                candidate.line_key.clone(),
                candidate.owner_user_id.clone(),
                candidate.rule_id.clone(),
                candidate.rule_type,
            )
            .map_err(Error::Logic)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use erp_procurement::entity::procurement_responsibility::ProcurementResponsibilityRuleType;

    use super::*;

    fn test_candidate(line_key: &str, owner: &str, owner_name: &str, rule_id: &str) -> CandidateResolution {
        test_candidate_with_type(line_key, owner, owner_name, rule_id, ProcurementResponsibilityRuleType::Sku)
    }

    /// 构造指定规则类型的测试候选责任.
    ///
    /// # 参数
    /// * `line_key` - 调用方稳定行键
    /// * `owner` - 负责人账号 ID
    /// * `owner_name` - 负责人展示姓名
    /// * `rule_id` - 命中规则 ID
    /// * `rule_type` - 命中规则类型
    ///
    /// # 返回
    /// 返回目录与账号资格已假设通过的候选责任.
    ///
    /// # 错误
    /// 无.
    ///
    /// # 约束
    /// 仅测试使用；不访问数据库.
    fn test_candidate_with_type(
        line_key: &str,
        owner: &str,
        owner_name: &str,
        rule_id: &str,
        rule_type: ProcurementResponsibilityRuleType,
    ) -> CandidateResolution {
        CandidateResolution {
            line_key: line_key.to_string(),
            owner_user_id: owner.to_string(),
            owner_name: owner_name.to_string(),
            rule_id: rule_id.to_string(),
            rule_type,
        }
    }

    // 授权行直接持有领域身份，视图映射只在最后一跳发生.断言身份字段与视图字段一致且计划不再持有 View.身份构造回归时测试失败.
    //
    // 不得再由 API View 反向构造领域身份.
    #[test]
    fn authorized_plan_holds_identity_and_maps_view_last_hop() {
        let line = authorized_line(test_candidate("line-1", "owner-1", "张三", "rule-1")).unwrap();
        assert_eq!(line.identity.line_key, "line-1");
        assert_eq!(line.identity.owner_user_id, "owner-1");
        assert_eq!(line.identity.rule_id, "rule-1");
        assert_eq!(line.owner_name, "张三");

        let plan = AuthorizedResolutionPlan { lines: vec![line], policy_revision: 7 };
        let views = plan.views();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].owner_name, "张三");
        assert_eq!(plan.identities().len(), 1);
    }

    // 展示姓名变化不影响计划身份比较.断言不同姓名的候选生成相同身份.身份误纳展示字段时测试失败.
    //
    // 姓名变化不得制造无效冲突.
    #[test]
    fn owner_name_change_does_not_change_identity() {
        let left = plan_identities(&[test_candidate("line-1", "owner-1", "张三", "rule-1")]).unwrap();
        let right = plan_identities(&[test_candidate("line-1", "owner-1", "李四", "rule-1")]).unwrap();
        assert_eq!(left, right);
    }

    // 行顺序、负责人、规则 ID 任一变化必须体现在身份差异中.断言三类变化均导致身份不等.身份遗漏关键字段时测试失败.
    //
    // 顺序变化必须拒绝；负责人或规则变化必须拒绝.
    #[test]
    fn order_owner_or_rule_change_changes_identity() {
        let base = test_candidate("line-1", "owner-1", "张三", "rule-1");
        let second = test_candidate("line-2", "owner-1", "张三", "rule-2");
        let ordered = plan_identities(&[base.clone(), second.clone()]).unwrap();
        let reordered = plan_identities(&[second, base]).unwrap();
        assert_ne!(ordered, reordered);

        let owner_changed =
            plan_identities(&[test_candidate("line-1", "owner-2", "张三", "rule-1")]).unwrap();
        assert_ne!(ordered[..1], owner_changed);

        let rule_changed = plan_identities(&[test_candidate("line-1", "owner-1", "张三", "rule-9")]).unwrap();
        assert_ne!(ordered[..1], rule_changed);
    }

    // 规则类型变化必须导致身份差异，重验比较必须拒绝.断言同行键同负责人同规则 ID 下 Sku 与 Category 生成不同身份，且授权计划的身份向量同样不等.
    //
    // 身份遗漏规则类型时测试失败.
    //
    // `claim_plan` 直接比较身份向量，身份不等即走 Conflict 回滚；展示姓名差异仍必须相等.
    #[test]
    fn rule_type_change_changes_identity_and_rejects_revalidation() {
        let sku = test_candidate_with_type(
            "line-1",
            "owner-1",
            "张三",
            "rule-1",
            ProcurementResponsibilityRuleType::Sku,
        );
        let category = test_candidate_with_type(
            "line-1",
            "owner-1",
            "张三",
            "rule-1",
            ProcurementResponsibilityRuleType::Category,
        );
        assert_ne!(
            plan_identities(std::slice::from_ref(&sku)).unwrap(),
            plan_identities(std::slice::from_ref(&category)).unwrap(),
            "规则类型变化必须拒绝"
        );

        let expected =
            AuthorizedResolutionPlan { lines: vec![authorized_line(sku).unwrap()], policy_revision: 7 };
        let actual = plan_identities(std::slice::from_ref(&test_candidate_with_type(
            "line-1",
            "owner-1",
            "张三",
            "rule-1",
            ProcurementResponsibilityRuleType::Category,
        )))
        .unwrap();
        assert_ne!(expected.identities(), actual, "重验身份向量不等必须触发 Conflict");
    }
}
