import { execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { API_BASE, apiGet, apiToken } from "./api";
import { ensurePersonWarehouseScope, savePersonScope, type PersonScopeView } from "./person-data-scope";

/** 沙箱仓储账号需要读取履约任务来源的销售组织，仅配置销售单详情。 */
export async function ensureWarehouseSalesSourceAccess(): Promise<void> {
  const config = process.env.ERP_E2E_CONFIG_PATH;
  if (process.env.ERP_E2E_ISOLATED !== "1" || !config) {
    throw new Error("仓储测试账号授权必须在隔离 E2E 环境配置");
  }
  const settings = JSON.parse(execFileSync("python3", ["-c", `
import json, sys, tomllib
config = tomllib.load(open(sys.argv[1], 'rb'))
print(json.dumps([config['database']['db_name'], config['app']['port']]))
`, config], { encoding: "utf8", timeout: 10_000 })) as [string, number];
  const origin = new URL(API_BASE);
  if (!/^erp_e2e_[0-9]{8}t[0-9]{6}_[0-9a-f]{12}_[0-9]+$/.test(settings[0]) ||
      origin.hostname !== "127.0.0.1" || Number(origin.port) !== settings[1]) {
    throw new Error("仓储测试账号授权必须由本次 E2E 分片的 API 执行");
  }
  const token = await apiToken("admin");
  const [directory, warehouseToken] = await Promise.all([
    apiGet<{ people: Array<{ id: string; account: string; own_org_unit_id: string | null }> }>(token, "/admin/org-units"),
    apiToken("cangchu"),
  ]);
  const warehouse = directory.people.find((person) => person.account === "cangchu");
  const salesOrganization = directory.people.find((person) => person.account === "xiaoshou")?.own_org_unit_id;
  if (!warehouse || !salesOrganization) throw new Error("缺少仓储账号或负责销售的测试组织");
  const profile = await apiGet<{ permissions: string[]; role_ids: string[] }>(warehouseToken, "/account/profile");
  const write = async (method: string, path: string, body: unknown) => {
    const response = await fetch(`${API_BASE}${path}`, {
      method,
      headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(15_000),
    });
    const result = await response.json() as { success: boolean; data: unknown; errorMessage?: string };
    if (!response.ok || !result.success) throw new Error(`${method} ${path}: ${result.errorMessage}`);
    return result.data;
  };
  if (!profile.permissions.some((permission) => ["*", "sales_order:*", "sales_order:detail"].includes(permission))) {
    const roleId = await write("POST", "/admin/roles", {
      name: `E2E 仓储销售来源 ${randomUUID().slice(0, 8)}`,
      permissions: ["sales_order:detail"],
    });
    if (typeof roleId !== "string") throw new Error("未返回仓储测试角色 ID");
    await write("PUT", `/admin/admins/${warehouse.id}/role`, { role_ids: [...profile.role_ids, roleId] });
  }
  const view = await apiGet<PersonScopeView>(token, `/admin/person-data-scopes/${warehouse.id}`);
  const existing = view.items.find((item) => item.resource === "sales_order" && item.action === "detail");
  if (existing?.expression.condition == null && existing?.expression.alternatives.some((terms) =>
    terms.length === 1 && terms[0]?.target_dimension === "internal_org" && (
      terms[0].scope_type === "company" ||
      (terms[0].scope_type === "organization" && terms[0].scope_targets.includes(salesOrganization))
    ),
  )) return;
  if (existing) throw new Error("仓储测试账号已有其他销售单范围；请保留原范围并检查夹具");
  await savePersonScope(token, warehouse.id, {
    resource: "sales_order",
    actions: ["detail"],
    grants: [{ actions: ["detail"], terms: [{
      scope_type: "organization",
      target_dimension: "internal_org",
      target_mode: "explicit",
      include_descendants: false,
      scope_targets: [salesOrganization],
    }] }],
    replace_legacy: true,
    expected_policy_version: view.policy_version,
  });
}

const WAREHOUSE_STOCK_GRANTS: ReadonlyArray<{
  account: string;
  resource: string;
  actions: readonly string[];
}> = [
  { account: "cangchu", resource: "stock_balance", actions: ["list", "detail"] },
  { account: "cangchu", resource: "stock_movement", actions: ["list"] },
  { account: "cangchu", resource: "stock_reservation", actions: ["list"] },
  {
    account: "cangchu",
    resource: "stock_adjustment",
    actions: ["list", "detail", "create", "update", "submit"],
  },
  { account: "caigou", resource: "stock_reservation", actions: ["list"] },
];

/**
 * 仓储人员没有库存范围时，台账停在「未配置仓库数据范围」。
 * 对实际岗位人员按仓库追加本流程范围，保留既有授权，不修改角色资格。
 */
export async function ensureWarehouseStockScope(warehouseCode: string): Promise<void> {
  const token = await apiToken("admin");
  const [warehouses, people] = await Promise.all([
    apiGet<{ items: Array<{ id: string; warehouse_code: string }> }>(
      token,
      "/admin/warehouses",
      { warehouse_code: warehouseCode, page: 1, page_size: 100 },
    ),
    apiGet<Array<{ id: string; account: string }>>(token, "/admin/admins"),
  ]);
  const warehouse = warehouses.items.find((row) => row.warehouse_code === warehouseCode);
  if (!warehouse) throw new Error(`缺少库存测试仓库：${warehouseCode}`);
  for (const account of new Set(WAREHOUSE_STOCK_GRANTS.map((grant) => grant.account))) {
    const person = people.find((row) => row.account === account);
    if (!person) throw new Error(`缺少库存测试人员：${account}`);
    await ensurePersonWarehouseScope(
      token,
      person.id,
      WAREHOUSE_STOCK_GRANTS.filter((grant) => grant.account === account),
      warehouse.id,
    );
  }
}

/** 只准备零数量仓库/SKU 维度；可用库存必须由浏览器提交盘盈并审批形成。 */
export async function ensureZeroBalanceDimension(
  warehouseCode: string,
  skuNo: string,
): Promise<void> {
  const config = process.env.ERP_E2E_CONFIG_PATH;
  if (process.env.ERP_E2E_ISOLATED !== "1" || !config) {
    throw new Error("库存测试维度必须使用本次隔离 E2E 配置");
  }
  const settings = JSON.parse(
    execFileSync(
      "python3",
      [
        "-c",
        "import json,sys,tomllib; print(json.dumps(tomllib.load(open(sys.argv[1], 'rb'))['database']))",
        config,
      ],
      { encoding: "utf8", timeout: 10_000 },
    ),
  ) as { uri: string; db_name: string };
  if (!/^erp_e2e_[0-9]{8}t[0-9]{6}_[0-9a-f]{12}_[0-9]+$/.test(settings.db_name)) {
    throw new Error("拒绝在 E2E 临时数据库以外准备库存测试维度");
  }
  const token = await apiToken("cangchu");
  const [warehouses, skus] = await Promise.all([
    apiGet<{ items: Array<{ id: string; warehouse_code: string }> }>(
      token,
      "/admin/warehouses",
      { warehouse_code: warehouseCode, page: 1, page_size: 100 },
    ),
    apiGet<{ items: Array<{ id: string; sku_no: string }> }>(
      token,
      "/admin/skus",
      { q: skuNo, page: 1, page_size: 100 },
    ),
  ]);
  const warehouse = warehouses.items.find((row) => row.warehouse_code === warehouseCode);
  const sku = skus.items.find((row) => row.sku_no === skuNo);
  if (!warehouse || !sku) throw new Error(`缺少库存测试主数据：${warehouseCode} / ${skuNo}`);
  const now = Math.floor(Date.now() / 1000);
  const script = `const target = db.getSiblingDB(${JSON.stringify(settings.db_name)});
        const key = {warehouse_id: ${JSON.stringify(warehouse.id)}, sku_id: ${JSON.stringify(sku.id)}, deleted_at: NumberLong(0)};
        const existing = target.stock_balances.findOne(key);
        if (!existing) {
          target.stock_balances.insertOne({...key,
            id: ${JSON.stringify(randomUUID().replaceAll("-", ""))}, version: NumberLong(1),
            created_at: NumberLong(${now}), updated_at: NumberLong(${now}),
            on_hand_quantity: NumberDecimal("0"), reserved_quantity: NumberDecimal("0"), available_quantity: NumberDecimal("0"), last_movement_id: null
          });
        } else {
          target.stock_balances.updateOne({_id: existing._id}, {$set: {
            updated_at: NumberLong(${now}),
            on_hand_quantity: NumberDecimal("0"), reserved_quantity: NumberDecimal("0"), available_quantity: NumberDecimal("0")
          }});
        }
        target.stock_reservations.deleteMany({warehouse_id: key.warehouse_id, sku_id: key.sku_id});`;
  try {
    execFileSync("mongosh", ["--norc", "--quiet", settings.uri, "--eval", script], {
      stdio: "pipe",
      timeout: 30_000,
    });
  } catch {
    throw new Error("零数量库存测试维度准备失败，请检查数据库连接和主数据");
  }
}

/** 预占台账按稳定销售明细标识展示；从本次销售单读取该标识，避免匹配其他单据。 */
export async function singleSalesLineId(salesOrderId: string): Promise<string> {
  const order = await apiGet<{ lines: Array<{ id: string }> }>(
    await apiToken("xiaoshou"), `/admin/sales-orders/${salesOrderId}`,
  );
  if (order.lines.length !== 1 || !order.lines[0].id) {
    throw new Error(`销售单 ${salesOrderId} 必须且仅有一条明细`);
  }
  return order.lines[0].id;
}
