import assert from "node:assert/strict";
import test from "node:test";
import { seedPersonDataScopes } from "./seed-person-data-scopes.mjs";

function business(resource, configurable_actions, default_self = true) {
  return { resource, configurable_actions, default_self, dimensions: ["internal_org"] };
}

function mockApi(t, businesses, initial = []) {
  const writes = [];
  const items = [...initial];
  let version = 10;
  t.mock.method(globalThis, "fetch", async (_url, options) => {
    if (options.method === "PUT") {
      const request = JSON.parse(options.body);
      assert.equal(request.expected_policy_version, version);
      assert.equal(request.replace_legacy, false);
      writes.push(request);
      items.push(...request.actions.map(action => ({ resource: request.resource, action })));
      version += 1;
    }
    return new Response(JSON.stringify({ success: true, data: { businesses, items, policy_version: version } }));
  });
  return writes;
}

test("采购保留本人商品范围，销售单追加公司查看，不为审批和财务来源写范围", async t => {
  const writes = mockApi(t, [business("product", ["list"]), business("sales_order", ["list", "detail"]), business("approval_instance", [], false), business("customer_receipt", [], false)]);
  await seedPersonDataScopes("test-token", { procurement: { id: "buyer" } });
  assert.equal(writes.length, 1);
  assert.equal(writes[0].resource, "sales_order");
  assert.equal(writes[0].grants[0].terms[0].scope_type, "company");
});

test("保留已有撤权，只补缺失动作，每次写入使用最新版本", async t => {
  const writes = mockApi(t, [business("sales_order", ["list", "detail"]), business("business_person", ["list"], false)], [{ resource: "sales_order", action: "list", expression: { alternatives: [] } }]);
  await seedPersonDataScopes("test-token", { procurement: { id: "buyer" } });
  assert.deepEqual(writes.map(row => row.actions), [["detail"], ["list"]]);
  assert.deepEqual(writes.map(row => row.expected_policy_version), [10, 11]);
  assert.equal(writes[1].grants[0].terms[0].scope_type, "company");
});
