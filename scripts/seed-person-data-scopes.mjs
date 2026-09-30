/** 显式演示人员配置；只补从未配置的业务动作，不改既有范围。 */
import { call } from "./dev-seed-lib.mjs";

export async function seedPersonDataScopes(token, accounts) {
  for (const [key, person] of Object.entries(accounts)) {
    const path = `/admin/person-data-scopes/${encodeURIComponent(person.id)}`;
    let view = await call("GET", path, { token });
    for (const business of view.businesses) {
      const actions = business.actions.filter(action => !view.items.some(scope => scope.resource === business.resource && scope.action === action));
      if (!actions.length) continue;
      const internal = business.dimensions.length === 1 && business.dimensions[0] === "internal_org";
      const self = internal && ["sales", "procurement", "operations"].includes(key);
      const ownOrg = internal && key === "salesLeader";
      const term = {
        scope_type: self ? "self_owned" : ownOrg ? "organization" : "company",
        target_dimension: business.dimensions[0],
        target_mode: ownOrg ? "own_org" : null,
        include_descendants: ownOrg ? true : null,
        scope_targets: [],
      };
      await call("PUT", path, { token, body: { resource: business.resource, actions, terms: [term], expected_policy_version: view.policy_version } });
      view = await call("GET", path, { token });
    }
  }
}
