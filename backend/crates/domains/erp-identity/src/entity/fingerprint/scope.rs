use super::Fingerprint;
use crate::entity::access_control::{ResolvedScope, ScopeClause};

impl Fingerprint {
    // 将授权范围编码进分页版本：先写无序角色条款，再写可选个人上限；未设上限不能与空条款混淆。
    pub(crate) fn scope(&mut self, scope: &ResolvedScope) {
        let ResolvedScope { role_clauses, user_limit } = scope;
        self.unordered(role_clauses, Self::clause);
        self.optional(user_limit.as_ref(), Self::clause);
    }

    // 将单个范围条款按固定顺序写入摘要：公司、本人、协作、组织、结算主体、仓库。
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
