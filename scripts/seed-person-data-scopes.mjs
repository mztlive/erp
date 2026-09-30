/** 显式演示追加范围；本人基础无需写入，只补从未配置的业务动作。 */
import { call, FOUNDATION } from "./dev-seed-lib.mjs";

export async function seedPersonDataScopes(token, accounts) {
  for (const [key, person] of Object.entries(accounts)) {
    const path = `/admin/person-data-scopes/${encodeURIComponent(person.id)}`;
    let view = await call("GET", path, { token });
    for (const business of view.businesses) {
      if (!Array.isArray(business.configurable_actions)) {
        throw new Error("人员范围接口缺少授权策略准入信息，请先更新后端");
      }
      const actions = business.configurable_actions.filter(action => !view.items.some(scope => scope.resource === business.resource && scope.action === action));
      if (!actions.length) continue;
      const internal = business.dimensions.length === 1 && business.dimensions[0] === "internal_org";
      const defaults = FOUNDATION.person_scope_defaults;
      const login = FOUNDATION.accounts.find(account => account.key === key)?.account;
      if (!login) throw new Error(`未声明的演示岗位 ${key}`);
      const company = defaults.company_resources[login]?.includes(business.resource) ?? false;
      const self = !company && internal && business.default_self && defaults.self_accounts.includes(login);
      const ownOrg = !company && internal && defaults.department_accounts.includes(login);
      if (self && business.default_self) continue;
      const term = {
        scope_type: self ? "self_owned" : ownOrg ? "organization" : "company",
        target_dimension: business.dimensions[0],
        target_mode: ownOrg ? "own_org" : null,
        include_descendants: ownOrg ? true : null,
        scope_targets: [],
      };
      await call("PUT", path, { token, body: {
        resource: business.resource,
        actions,
        grants: [{ actions, terms: [term] }],
        replace_legacy: false,
        expected_policy_version: view.policy_version,
      } });
      view = await call("GET", path, { token });
    }
  }
}
