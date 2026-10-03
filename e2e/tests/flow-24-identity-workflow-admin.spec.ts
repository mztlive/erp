/**
 * [flow-24] 人员与角色实际开通、审批定义版本发布及退役。
 * 使用隔离 shard 的真实 UI、HTTP 与数据库；角色资格、数据归属和审批绑定分别断言。
 */
import { randomUUID } from "node:crypto";

import { expect, test, type Page, type Response as BrowserResponse } from "../helpers/test";
import { API_BASE, apiGet, apiToken } from "../helpers/api";
import { createCustomerViaUi } from "../helpers/customers";
import { ensureWarehouseStockScope, ensureZeroBalanceDimension } from "../helpers/inventory";
import { loginViaUi } from "../helpers/login";

const VISIBLE = { timeout: 30_000 } as const;

type Envelope<T> = { success?: boolean; errorMessage?: string; data: T };
type Account = { id: string; account: string; name: string; role_ids: string[] };
type Role = { id: string; name: string; permissions: string[] };
type BuiltinRoleCatalog = {
  policy_version: number;
  templates: Array<{
    id: string;
    name: string;
    permissions: string[];
    recommended: boolean;
    can_generate: boolean;
  }>;
};
type Node = {
  node_id: string;
  node_key: string;
  node_name: string;
  display_order: number;
  assignee_user_id: string;
};
type Definition = {
  definition_id: string;
  definition_version: number;
  definition_lock_version: number;
  status: "DRAFT" | "PUBLISHED" | "RETIRED";
  name: string;
  nodes: Node[];
};
type Adjustment = {
  adjustment: { id: string };
  approval: {
    definition: { id: string; version: number };
    submit_command: { expected_version: string; expected_subject_version: string };
  };
  lines: Array<{ id: string }>;
};

async function call<T>(method: string, path: string, token: string, body?: unknown) {
  const response = await fetch(`${API_BASE}${path}`, {
    method,
    headers: {
      Authorization: `Bearer ${token}`,
      ...(body === undefined ? {} : { "Content-Type": "application/json" }),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(15_000),
  });
  const parsed = (await response.json()) as Envelope<T>;
  return { status: response.status, parsed };
}

async function ok<T>(method: string, path: string, token: string, body?: unknown): Promise<T> {
  const reply = await call<T>(method, path, token, body);
  expect(reply.status, `${method} ${path}: ${reply.parsed.errorMessage ?? ""}`).toBeLessThan(400);
  expect(reply.parsed.success, `${method} ${path}: ${reply.parsed.errorMessage ?? ""}`).not.toBe(
    false,
  );
  return reply.parsed.data;
}

async function uiWrite<T>(
  page: Page,
  method: string,
  path: string,
  action: () => Promise<unknown>,
): Promise<T> {
  const [response] = await Promise.all([
    page.waitForResponse(
      (reply) => reply.request().method() === method && new URL(reply.url()).pathname === path,
    ),
    action(),
  ]);
  return browserResult<T>(response);
}

async function browserResult<T>(response: BrowserResponse): Promise<T> {
  const body = (await response.json()) as Envelope<T>;
  expect(response.status(), `${response.url()}: ${body.errorMessage ?? ""}`).toBeLessThan(400);
  expect(body.success, body.errorMessage).not.toBe(false);
  return body.data;
}

test("[flow-24] 角色、人员统一保存与权限撤销不会扩大客户数据范围", async ({ browser }) => {
  const suffix = Date.now().toString(36);
  const roleName = `E2E 客户查看 ${suffix}`;
  const accountName = `e24_${suffix}`;
  const initialName = `E2E 新人员 ${suffix}`;
  const updatedName = `E2E 已核对 ${suffix}`;
  const adminContext = await browser.newContext();
  const admin = await adminContext.newPage();
  await loginViaUi(admin, "admin");
  const token = await apiToken("admin");

  await test.step("管理员在角色页面创建受限角色并在人员页面创建账号", async () => {
    await admin.goto("/system/roles/new");
    await admin.locator("#governance-admin-role-form-name").fill(roleName);
    await admin.locator("#governance-admin-role-form-permissions-search").fill("客户");
    await admin.getByRole("checkbox", { name: "客户 · 查看列表", exact: true }).check();
    await uiWrite(admin, "POST", "/admin/roles", () =>
      admin.locator("#governance-admin-role-form-submit").click(),
    );
  });
  const role = (await apiGet<Role[]>(token, "/admin/roles")).find((row) => row.name === roleName);
  expect(role).toBeTruthy();
  expect(role!.permissions).toEqual(["customer:list"]);

  await admin.goto("/system/accounts");
  await admin.locator("#governance-admin-accounts-create").click();
  const dialog = admin.getByRole("dialog", { name: "新建账号" });
  await dialog.locator("#governance-admin-account-dialog-account").fill(accountName);
  await dialog.locator("#governance-admin-account-dialog-name").fill(initialName);
  await dialog.locator("#governance-admin-account-dialog-password").fill("123456");
  await dialog
    .getByRole("checkbox", { name: new RegExp(`^${roleName}`) })
    .first()
    .check();
  await uiWrite(admin, "POST", "/admin/admins", () =>
    dialog.locator("#governance-admin-account-dialog-submit").click(),
  );
  await expect(dialog).not.toBeVisible(VISIBLE);
  const account = (await apiGet<Account[]>(token, "/admin/admins")).find(
    (row) => row.account === accountName,
  );
  expect(account).toBeTruthy();
  expect(account!.role_ids).toEqual([role!.id]);

  await test.step("人员资料通过预览统一保存，刷新后姓名和角色持久化", async () => {
    await admin.goto(`/system/accounts/${account!.id}`);
    await admin.locator("#account-profile-edit").click();
    await admin.locator("#account-profile-name").fill(updatedName);
    await admin.locator("#account-profile-reason").fill("E2E 核对人员资料统一提交");
    await uiWrite(admin, "POST", "/admin/org-units/preview", () =>
      admin.locator("#account-profile-save").click(),
    );
    await expect(admin.getByText("请确认本次修改", { exact: true })).toBeVisible(VISIBLE);
    await uiWrite(admin, "POST", "/admin/org-units/change", () =>
      admin.locator("#account-profile-save").click(),
    );
    await admin.reload();
    await expect(admin.getByText(updatedName, { exact: true }).first()).toBeVisible(VISIBLE);
    const saved = (await apiGet<Account[]>(token, "/admin/admins")).find(
      (row) => row.id === account!.id,
    );
    expect(saved?.name).toBe(updatedName);
    expect(saved?.role_ids).toEqual([role!.id]);
  });

  const salesContext = await browser.newContext();
  const sales = await salesContext.newPage();
  await loginViaUi(sales, "xiaoshou");
  const customerName = `E2E 他人负责客户 ${suffix}`;
  await createCustomerViaUi(sales, {
    legalName: customerName,
    paymentTermLabel: "货到 15 天",
    contact: { name: "验收联系人", phone: "13800138024" },
    address: "北京市朝阳区验收路 24 号",
  });
  const salesToken = await apiToken("xiaoshou");
  const salesCustomers = await apiGet<{ items: Array<{ id: string; legal_name: string }> }>(
    salesToken,
    "/admin/customers",
    { keyword: customerName, page: 1, page_size: 100 },
  );
  expect(salesCustomers.items.some((row) => row.legal_name === customerName)).toBe(true);

  await test.step("本人真实登录获得角色动作；他人负责客户仍不可见；角色撤权即时生效", async () => {
    const personContext = await browser.newContext();
    const person = await personContext.newPage();
    // 此角色没有工作台读取权；真实登录应进入已授权的客户列表。
    await person.goto("/login?returnTo=%2Fsales%2Fcustomers");
    await person.locator("#governance-auth-login-account").fill(accountName);
    await person.locator("#governance-auth-login-password").fill("123456");
    const signedIn = await uiWrite<{ token: string }>(person, "POST", "/login", () =>
      person.locator("#governance-auth-login-submit").click(),
    );
    await person.waitForURL((url) => url.pathname === "/sales/customers");
    await expect(person.getByRole("heading", { name: "客户中心", exact: true })).toBeVisible(
      VISIBLE,
    );
    const personToken = signedIn.token;
    const visible = await apiGet<{ items: Array<{ id: string }> }>(
      personToken,
      "/admin/customers",
      { keyword: customerName, page: 1, page_size: 100 },
    );
    expect(visible.items).toEqual([]);
    await admin.goto(`/system/roles/${role!.id}/edit`);
    await admin.getByRole("checkbox", { name: "客户 · 查看列表", exact: true }).uncheck();
    await uiWrite(admin, "PUT", `/admin/roles/${role!.id}`, () =>
      admin.locator("#governance-admin-role-form-submit").click(),
    );
    expect((await call("GET", "/admin/customers", personToken)).status).toBe(403);
    await personContext.close();
  });

  await test.step("销售不能绕过页面创建角色、人员或审批定义", async () => {
    const denied = [
      await call("POST", "/admin/roles", salesToken, { name: "销售越权角色", permissions: [] }),
      await call("POST", "/admin/admins", salesToken, {
        account: `denied_${suffix}`,
        name: "越权",
        password: "123456",
        role_ids: [role!.id],
      }),
      await call("POST", "/admin/approval-process-definitions/drafts", salesToken, {
        document_type: "stock_adjustment",
        name: "越权定义",
        draft_source: "EMPTY",
        idempotency_key: randomUUID(),
      }),
    ];
    expect(denied.map((reply) => reply.status)).toEqual([403, 403, 403]);
    await sales.goto("/system/approval-processes/stock_adjustment");
    await expect(sales.getByText("当前账号不能查看审批流程定义。", { exact: true })).toBeVisible(
      VISIBLE,
    );
  });

  await test.step("内建岗位补齐缺失角色，重复生成保留既有角色权限", async () => {
    const before = (await apiGet<Role[]>(token, "/admin/roles")).find(
      (row) => row.id === "role-sales",
    )!;
    const catalog = await apiGet<BuiltinRoleCatalog>(token, "/admin/role-templates");
    const missing = catalog.templates.filter((row) => row.recommended && row.can_generate);
    expect(missing.length, "隔离源库保留旧财务角色，分岗内建模板须可补齐").toBeGreaterThan(0);
    await admin.goto("/system/access-audit?view=roles");
    await admin.locator("#roles-builtin-open").click();
    const dialog = admin.getByRole("dialog").filter({
      has: admin.getByRole("heading", { name: "生成岗位角色", exact: true }),
    });
    await expect(dialog.getByRole("checkbox", { name: "销售", exact: true })).toBeDisabled();
    const generated = await uiWrite<Array<{ id: string; created: boolean }>>(
      admin,
      "POST",
      "/admin/role-templates/generate",
      () => admin.locator("#roles-builtin-generate").click(),
    );
    expect(generated.map((row) => row.id).sort()).toEqual(missing.map((row) => row.id).sort());
    expect(generated.every((row) => row.created)).toBe(true);
    const roles = await apiGet<Role[]>(token, "/admin/roles");
    for (const template of missing) {
      expect(
        roles
          .find((row) => row.id === template.id)
          ?.permissions.slice()
          .sort(),
      ).toEqual(template.permissions.slice().sort());
    }
    const refreshed = await apiGet<BuiltinRoleCatalog>(token, "/admin/role-templates");
    const repeated = await ok<Array<{ id: string; created: boolean }>>(
      "POST",
      "/admin/role-templates/generate",
      token,
      { template_ids: ["role-sales"], expected_policy_version: refreshed.policy_version },
    );
    expect(repeated).toContainEqual(expect.objectContaining({ id: "role-sales", created: false }));
    const after = (await apiGet<Role[]>(token, "/admin/roles")).find(
      (row) => row.id === "role-sales",
    )!;
    expect(after).toEqual(before);
  });
  await salesContext.close();
  await adminContext.close();
});

test("[flow-24] 审批草稿校验、发布、退役保持旧单据绑定及历史定义", async ({ page }) => {
  const suffix = Date.now().toString(36);
  await loginViaUi(page, "admin");
  const token = await apiToken("admin");
  const beforeVersions = await apiGet<Definition[]>(
    token,
    "/admin/approval-processes/stock_adjustment/versions",
  );
  const originalVersion = beforeVersions.find((row) => row.status === "PUBLISHED")!;
  expect(originalVersion).toBeTruthy();
  const original = await apiGet<Definition>(
    token,
    `/admin/approval-process-definitions/${originalVersion.definition_id}`,
  );

  await ensureWarehouseStockScope("BJ-TZ-01");
  await ensureZeroBalanceDimension("BJ-TZ-01", "TEA-SF-LJ-250");
  const warehouseToken = await apiToken("cangchu");
  const warehouses = await apiGet<{ items: Array<{ id: string; warehouse_code: string }> }>(
    warehouseToken,
    "/admin/warehouses",
    { page: 1, page_size: 100 },
  );
  const warehouse = warehouses.items.find((row) => row.warehouse_code === "BJ-TZ-01")!;
  const skus = await apiGet<{ items: Array<{ id: string; sku_no: string }> }>(
    warehouseToken,
    "/admin/skus",
    { q: "TEA-SF-LJ-250", page: 1, page_size: 100 },
  );
  const sku = skus.items.find((row) => row.sku_no === "TEA-SF-LJ-250")!;
  const balances = await apiGet<{
    items: Array<{ id: string; warehouse_id: string; sku_id: string; version: string }>;
  }>(warehouseToken, "/admin/stock-balances", { page: 1, page_size: 100 });
  const balance = balances.items.find(
    (row) => row.warehouse_id === warehouse.id && row.sku_id === sku.id,
  )!;
  expect(balance).toBeTruthy();
  const createBody = {
    balance_id: balance.id,
    expected_balance_version: balance.version,
    adjustment_no: `E24-${suffix}`,
    warehouse_id: warehouse.id,
    reason_type: "STOCK_GAIN",
    lines: [{ sku_id: sku.id, quantity: "1", direction: "INCREASE" }],
    note: "E2E 定义绑定保持",
    occurred_at: Math.floor(Date.now() / 1000),
  };
  const bound = await ok<Adjustment>(
    "POST",
    "/admin/stock-adjustments",
    warehouseToken,
    createBody,
  );
  expect(bound.approval.definition.id).toBe(original.definition_id);

  let draft: Definition;
  try {
    await test.step("复制当前版本创建草稿；来源和节点名称校验保留草稿", async () => {
      await page.goto("/system/approval-processes/stock_adjustment");
      await page.locator("#governance-approval-processes-detail-create-draft").click();
      const dialog = page.getByRole("dialog", { name: "新建草稿" });
      await dialog
        .locator("#governance-approval-processes-detail-create-draft-dialog-name")
        .fill(`E2E 库存审批 ${suffix}`);
      await dialog
        .locator("#governance-approval-processes-detail-create-draft-dialog-submit")
        .click();
      await expect(dialog.getByText("请选择草稿来源", { exact: true })).toBeVisible(VISIBLE);
      await dialog
        .locator(
          "#governance-approval-processes-detail-create-draft-dialog-draft-source-current-published",
        )
        .check();
      draft = await uiWrite<Definition>(
        page,
        "POST",
        "/admin/approval-process-definitions/drafts",
        () =>
          dialog
            .locator("#governance-approval-processes-detail-create-draft-dialog-submit")
            .click(),
      );
      expect(draft.status).toBe("DRAFT");
      expect(draft.nodes.map((row) => row.assignee_user_id)).toEqual(
        original.nodes.map((row) => row.assignee_user_id),
      );
      const name = page
        .locator('[id^="governance-approval-processes-detail-editor-nodes-node-"][id$="-name"]')
        .first();
      await name.fill("");
      await expect(
        page.locator("#governance-approval-processes-detail-editor-save"),
      ).toBeDisabled();
      expect(
        (
          await apiGet<Definition>(
            token,
            `/admin/approval-process-definitions/${draft.definition_id}`,
          )
        ).definition_lock_version,
      ).toBe(draft.definition_lock_version);
      await name.fill(`E2E 财务复核 ${suffix}`);
      await expect(page.locator("#governance-approval-processes-detail-editor-save")).toBeEnabled();
      draft = await uiWrite<Definition>(
        page,
        "PUT",
        `/admin/approval-process-definitions/${draft.definition_id}/nodes`,
        () => page.locator("#governance-approval-processes-detail-editor-save").click(),
      );
      expect(draft.nodes[0]!.node_name).toBe(`E2E 财务复核 ${suffix}`);
    });

    await test.step("发布新版本退役旧定义；旧草稿仍绑定创建时版本；已发布定义不可改写", async () => {
      await page.locator("#governance-approval-processes-detail-publish").click();
      const publishDialog = page.getByRole("dialog", { name: "发布审批流程" });
      await expect(
        publishDialog.getByText("发布后当前已发布版本将退役。已绑定单据和进行中的审批不受影响。", {
          exact: true,
        }),
      ).toBeVisible(VISIBLE);
      const published = await uiWrite<Definition>(
        page,
        "POST",
        `/admin/approval-process-definitions/${draft!.definition_id}/publish`,
        () =>
          publishDialog
            .locator("#governance-approval-processes-detail-publish-dialog-confirm")
            .click(),
      );
      expect(published.status).toBe("PUBLISHED");
      const retired = await apiGet<Definition>(
        token,
        `/admin/approval-process-definitions/${original.definition_id}`,
      );
      expect(retired.status).toBe("RETIRED");
      expect(retired.nodes).toEqual(original.nodes);
      const unchanged = await apiGet<Adjustment>(
        warehouseToken,
        `/admin/stock-adjustments/${bound.adjustment.id}`,
      );
      expect(unchanged.approval.definition.id).toBe(original.definition_id);
      expect(unchanged.approval.definition.version).toBe(original.definition_version);
      const rejected = await call(
        "PUT",
        `/admin/approval-process-definitions/${published.definition_id}/nodes`,
        token,
        {
          expected_definition_lock_version: String(published.definition_lock_version),
          nodes: published.nodes.map((node) => ({
            node_id: node.node_id,
            node_name: "禁止改写",
            display_order: node.display_order,
            assignee_user_id: node.assignee_user_id,
          })),
          idempotency_key: randomUUID(),
        },
      );
      expect(rejected.status).toBeGreaterThanOrEqual(400);
      expect(
        (
          await apiGet<Definition>(
            token,
            `/admin/approval-process-definitions/${published.definition_id}`,
          )
        ).nodes,
      ).toEqual(published.nodes);
      const newBound = await ok<Adjustment>("POST", "/admin/stock-adjustments", warehouseToken, {
        ...createBody,
        adjustment_no: `E24-NEW-${suffix}`,
      });
      expect(newBound.approval.definition.id).toBe(published.definition_id);
      await page.goto("/system/approval-processes/stock_adjustment");
      await expect(page.getByRole("heading", { name: published.name, exact: true })).toBeVisible(
        VISIBLE,
      );
      await expect(page.locator("#governance-approval-processes-detail-editor-save")).toHaveCount(
        0,
      );
      await page.locator("#governance-approval-processes-detail-retire").click();
      await uiWrite<Definition>(
        page,
        "POST",
        `/admin/approval-process-definitions/${published.definition_id}/retire`,
        () => page.locator("#governance-approval-processes-retire-dialog-confirm").click(),
      );
      await expect(page.getByText("配置缺失", { exact: true }).first()).toBeVisible(VISIBLE);
      const refused = await call("POST", "/admin/stock-adjustments", warehouseToken, {
        ...createBody,
        adjustment_no: `E24-REFUSED-${suffix}`,
      });
      expect(refused.status, refused.parsed.errorMessage).toBeGreaterThanOrEqual(400);
      expect(refused.parsed.errorMessage).toMatch(/审批|配置|发布/);
      const kept = await apiGet<Adjustment>(
        warehouseToken,
        `/admin/stock-adjustments/${bound.adjustment.id}`,
      );
      expect(kept.approval.definition.id).toBe(original.definition_id);
    });
  } finally {
    // 同一 shard 后续流程继续使用原审批人及节点；已发布版本不可重新启用，
    // 因此通过正式定义接口发布内容等价的新版本，保留本轮全部历史。
    const versions = await apiGet<Definition[]>(
      token,
      "/admin/approval-processes/stock_adjustment/versions",
    );
    const active = versions.find((row) => row.status === "PUBLISHED");
    if (
      active?.definition_id !== original.definition_id ||
      versions.some((row) => row.status === "DRAFT")
    ) {
      let restore = versions.find((row) => row.status === "DRAFT")
        ? await apiGet<Definition>(
            token,
            `/admin/approval-process-definitions/${versions.find((row) => row.status === "DRAFT")!.definition_id}`,
          )
        : await ok<Definition>("POST", "/admin/approval-process-definitions/drafts", token, {
            document_type: "stock_adjustment",
            name: original.name,
            draft_source: "EMPTY",
            idempotency_key: randomUUID(),
          });
      restore = await ok<Definition>(
        "PUT",
        `/admin/approval-process-definitions/${restore.definition_id}/nodes`,
        token,
        {
          expected_definition_lock_version: String(restore.definition_lock_version),
          nodes: original.nodes.map((node) => ({
            node_name: node.node_name,
            display_order: node.display_order,
            assignee_user_id: node.assignee_user_id,
          })),
          idempotency_key: randomUUID(),
        },
      );
      const restored = await ok<Definition>(
        "POST",
        `/admin/approval-process-definitions/${restore.definition_id}/publish`,
        token,
        {
          expected_definition_lock_version: String(restore.definition_lock_version),
          idempotency_key: randomUUID(),
        },
      );
      expect(restored.status).toBe("PUBLISHED");
      expect(
        restored.nodes.map(({ node_name, display_order, assignee_user_id }) => ({
          node_name,
          display_order,
          assignee_user_id,
        })),
      ).toEqual(
        original.nodes.map(({ node_name, display_order, assignee_user_id }) => ({
          node_name,
          display_order,
          assignee_user_id,
        })),
      );
    }
  }
});
