import assert from "node:assert/strict";
import { test } from "node:test";
import { ACCOUNTS, FOUNDATION, syncIsolatedDevRolePermissions } from "./dev-seed-lib.mjs";

/** 纯 HTTP 替身核对隔离岗位准备；不访问服务端或复制源库。 */
function fixture() {
  const ids = [...new Set(Object.values(ACCOUNTS).map((account) => account.roleId))];
  const catalog = { templates: ids.map((id) => ({ id, permissions: ["work_item:detail", ...(id === "role-cashier" ? ["purchase_payment_request:detail", "purchase_payment_request:pay"] : [])] })) };
  const roles = ids.map((id) => ({ id, permissions: ["legacy:read"] }));
  const writes = [];
  const transport = async (method, path, options) => writes.push({ method, path, ...options });
  return { catalog, roles, writes, transport };
}

test("复制源库的旧出纳岗位同步正式 PPR 执行资格，其他源角色不得扩大", async () => {
  const api = fixture();
  api.roles.push({ id: "custom-source-role", permissions: ["custom:read"] });
  const unrelated = structuredClone(api.roles.at(-1));
  await syncIsolatedDevRolePermissions("admin", api.catalog, api.roles, api.transport);
  const cashier = api.roles.find((row) => row.id === "role-cashier");
  assert.ok(cashier.permissions.includes("purchase_payment_request:detail"));
  assert.ok(cashier.permissions.includes("purchase_payment_request:pay"));
  assert.ok(!cashier.permissions.includes("legacy:read"));
  for (const account of FOUNDATION.accounts) {
    const role = api.roles.find((row) => row.id === account.role_id);
    for (const required of FOUNDATION.required_permissions[account.key]) assert.ok(role.permissions.includes(required));
  }
  assert.deepEqual(api.roles.at(-1), unrelated);
  assert.ok(api.writes.every((write) => write.method === "PUT" && !write.path.endsWith("custom-source-role")));
  const before = api.writes.length;
  await syncIsolatedDevRolePermissions("admin", api.catalog, api.roles, api.transport);
  assert.equal(api.writes.length, before, "同一权限合同重跑零写入");
});

test("缺失模板立即拒绝，不能用旧角色绕过新执行资格", async () => {
  const api = fixture();
  api.catalog.templates = [];
  await assert.rejects(syncIsolatedDevRolePermissions("admin", api.catalog, api.roles, api.transport), /缺少服务器模板/);
  assert.equal(api.writes.length, 0);
});
