use super::Fingerprint;
use crate::entity::access_control::{ResolvedScope, ScopeClause};

impl Fingerprint {
    /// 为版本绑定范围：先编码无序角色条款，再编码可选个人上限，区分未设上限与空条款。
    pub(crate) fn scope(&mut self, scope: &ResolvedScope) {
        let ResolvedScope { role_clauses, user_limit } = scope;
        self.unordered(role_clauses, Self::clause);
        self.optional(user_limit.as_ref(), Self::clause);
    }

    // 依次编码公司、本人、协作及组织、结算主体、仓库，三组 ID 均按无序集合处理。
    fn clause(&mut self, clause: &ScopeClause) {
        let ScopeClause {
            company,
            self_owned,
            collaborative,
            org_unit_ids,
            settlement_party_ids,
            warehouse_ids,
        } = clause;
        self.boolean(*company);
        self.boolean(*self_owned);
        self.boolean(*collaborative);
        for ids in [org_unit_ids, settlement_party_ids, warehouse_ids] {
            self.unordered(ids, |hash, id| hash.text(id));
        }
    }
}
