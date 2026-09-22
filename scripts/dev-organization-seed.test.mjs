import assert from "node:assert/strict";
import { test } from "node:test";
import { ensureDevOrganization, DEV_DEPARTMENTS } from "./dev-organization-seed.mjs";

const accounts = DEV_DEPARTMENTS.flatMap((department) => department.accounts.map((account) => ({ id: `u-${account}`, account, role_ids: account === "lisiyong" ? ["role-sales-leader"] : [] })));

/** HTTP 替身记录用例调用并返回变更回执，不连接数据库。 */
function fixture(initial = {}) {
  let state = { version: 1, units: [], memberships: [], management: [], ...initial };
  const writes = [];
  const transport = async (method, path, { body } = {}) => {
    if (method === "GET") return structuredClone(state);
    assert.equal(body.expected_version, state.version);
    if (path.endsWith("/preview")) return {};
    writes.push(body);
    const change = body.change;
    const id = `row-${writes.length}`;
    if (change.operation === "create_unit") state.units.push({ ...change, id, enabled: true });
    if (change.operation === "transfer_member") {
      state.memberships = state.memberships.map((row) => row.user_id === change.user_id && row.valid_to == null ? { ...row, valid_to: 100 } : row);
      state.memberships.push({ ...change, id, valid_from: 100, valid_to: null });
    }
    if (change.operation === "grant_management") state.management.push({ ...change, id, valid_from: 100 });
    if (change.operation === "revoke_management") state.management = state.management.map((row) => row.id === change.assignment_id ? { ...row, valid_to: 100 } : row);
    state.version += 1;
    return { after: structuredClone(state) };
  };
  return { transport, writes, state: () => state };
}

test("首次补齐全部岗位部门、11 个账号和销售领导管理部门；重跑零写入", async () => {
  const api = fixture();
  await ensureDevOrganization("test", { transport: api.transport, accounts, now: 100 });
  assert.equal(api.state().units.length, 8);
  assert.equal(api.state().memberships.length, 11);
  for (const spec of DEV_DEPARTMENTS) {
    const unit = api.state().units.find((row) => row.name === spec.name);
    assert.deepEqual(api.state().memberships.filter((row) => row.org_unit_id === unit.id).map((row) => row.user_id).sort(), spec.accounts.map((account) => `u-${account}`).sort());
  }
  assert.equal(api.state().management[0].role_id, "role-sales-leader");
  assert.equal(api.state().management[0].org_unit_id, api.state().units.find((row) => row.name === "销售部").id);
  assert.equal(api.state().management[0].include_descendants, true);
  const before = api.writes.length;
  await ensureDevOrganization("test", { transport: api.transport, accounts, now: 100 });
  assert.equal(api.writes.length, before);
});

test("复用已有销售部；调岗通过变更用例结束旧归属", async () => {
  const api = fixture({ units: [{ id: "root", name: "总部", parent_id: null, kind: "department", enabled: true }, { id: "sales", name: "销售部", parent_id: "root", kind: "department", enabled: true }], memberships: [{ id: "old", user_id: "u-xiaoshou", org_unit_id: "root", valid_from: 0, valid_to: null }] });
  await ensureDevOrganization("test", { transport: api.transport, accounts, now: 100 });
  assert.equal(api.state().units.filter((row) => row.name === "销售部").length, 1);
  assert.equal(api.state().memberships.find((row) => row.id === "old").valid_to, 100);
  assert.equal(api.state().memberships.find((row) => row.user_id === "u-xiaoshou" && row.valid_to == null).org_unit_id, "sales");
});

test("缺账号拒绝写入；停用部门拒绝复用", async () => {
  const missing = fixture();
  await assert.rejects(ensureDevOrganization("test", { transport: missing.transport, accounts: [], now: 100 }), /缺少或重复种子账号/);
  assert.equal(missing.writes.length, 0);
  const disabled = fixture({ units: [{ id: "root", name: "总部", parent_id: null, kind: "department", enabled: false }] });
  await assert.rejects(ensureDevOrganization("test", { transport: disabled.transport, accounts, now: 100 }), /已停用或类型不匹配/);
});

test("预览冲突立即停止，不提交也不重试", async () => {
  let calls = 0;
  const transport = async (method) => {
    calls++;
    if (method === "GET") return { version: 2, units: [], memberships: [], management: [] };
    throw new Error("组织版本已变化");
  };
  await assert.rejects(ensureDevOrganization("test", { transport, accounts, now: 100 }), /组织版本已变化/);
  assert.equal(calls, 2);
});
