/** 开发/E2E 组织种子：通过组织变更用例维护部门与主属关系。 */
import { randomUUID } from "node:crypto";
import { call, FOUNDATION, listAdmins } from "./dev-seed-lib.mjs";

export const DEV_DEPARTMENTS = FOUNDATION.departments.map(({ name, accounts }) => ({ name, accounts }));

/**
 * 补齐岗位部门。仅调整目录内的种子账号，不交接既有客户、单据或待办。
 * 重复运行复用部门与有效关系；版本冲突或提交失败立即停止，不自动重放写入。
 * transport/accounts/now 可注入以离线验证真实编排。
 */
export async function ensureDevOrganization(token, options = {}) {
  const transport = options.transport ?? call;
  const accounts = options.accounts ?? await listAdmins(token);
  const now = options.now ?? Math.floor(Date.now() / 1000);
  let state = await transport("GET", "/admin/org-units", { token });
  const active = (row) => row.valid_from <= now && (row.valid_to == null || now < row.valid_to);
  const change = async (operation) => {
    const request = {
      expected_version: state.version,
      idempotency_key: `dev-organization-${randomUUID()}`,
      reason: "开发与 E2E 岗位部门初始化",
      change: operation,
    };
    await transport("POST", "/admin/org-units/preview", { token, body: request });
    const receipt = await transport("POST", "/admin/org-units/change", { token, body: request });
    state = receipt.after;
  };
  const unit = async (name, parentId) => {
    const matches = state.units.filter((row) => row.name === name && (row.parent_id ?? null) === parentId);
    if (matches.length > 1) throw new Error(`部门 ${name} 重名，请先合并后重新初始化`);
    if (matches.length === 1) {
      if (!matches[0].enabled || matches[0].kind !== "department") throw new Error(`部门 ${name} 已停用或类型不匹配`);
      return matches[0];
    }
    await change({ operation: "create_unit", name, parent_id: parentId, kind: "department" });
    const created = state.units.find((row) => row.name === name && (row.parent_id ?? null) === parentId);
    if (!created) throw new Error(`创建 ${name} 后未返回部门`);
    return created;
  };
  // 先核对人员，避免漏建账号时只写入一半部门关系。
  for (const spec of DEV_DEPARTMENTS) {
    for (const name of spec.accounts) {
      if (accounts.filter((account) => account.account === name).length !== 1) throw new Error(`缺少或重复种子账号 ${name}`);
    }
  }
  const root = await unit(FOUNDATION.root_department, null);
  for (const spec of DEV_DEPARTMENTS) {
    const department = await unit(spec.name, root.id);
    for (const name of spec.accounts) {
      const account = accounts.find((row) => row.account === name);
      const memberships = state.memberships.filter((row) => row.user_id === account.id && active(row));
      if (memberships.length > 1) throw new Error(`${name} 存在多个有效所属部门`);
      if (memberships[0]?.org_unit_id === department.id) continue;
      await change({ operation: "transfer_member", user_id: account.id, org_unit_id: department.id });
    }
  }
  return state;
}
