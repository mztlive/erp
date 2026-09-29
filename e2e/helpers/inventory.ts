import { execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { fileURLToPath } from "node:url";
import { API_BASE, apiGet, apiToken } from "./api";

type StockScopeRow = {
  enabled?: boolean;
  scope_type?: string;
  scope_targets?: string[];
  actions?: string[];
};

const WAREHOUSE_STOCK_GRANTS: ReadonlyArray<{
  roleId: string;
  resource: string;
  actions: readonly string[];
}> = [
  { roleId: "role-warehouse", resource: "stock_balance", actions: ["list", "detail"] },
  { roleId: "role-warehouse", resource: "stock_movement", actions: ["list"] },
  { roleId: "role-warehouse", resource: "stock_reservation", actions: ["list"] },
  {
    roleId: "role-warehouse",
    resource: "stock_adjustment",
    actions: ["list", "detail", "create", "update", "submit"],
  },
  { roleId: "role-procurement", resource: "stock_reservation", actions: ["list"] },
];

async function apiPost<T>(token: string, path: string, body: unknown): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body),
    signal: AbortSignal.timeout(15_000),
  });
  const text = await response.text();
  const parsed = text
    ? (JSON.parse(text) as { success?: boolean; errorMessage?: string; data?: T })
    : null;
  if (!response.ok || parsed?.success === false || parsed?.data == null) {
    throw new Error(
      `API POST ${path} 失败（HTTP ${response.status}）: ${parsed?.errorMessage ?? text.slice(0, 300)}`,
    );
  }
  return parsed.data;
}

function scopeCovers(row: StockScopeRow, actions: readonly string[], warehouseId: string): boolean {
  if (row.enabled === false) return false;
  const granted = new Set(row.actions ?? []);
  if (!actions.every((action) => granted.has(action))) return false;
  if (row.scope_type === "company") return true;
  const targets = row.scope_targets ?? [];
  return targets.includes(warehouseId) || targets.includes("*");
}

/**
 * 仓储角色没有库存默认范围时，台账停在「未配置仓库数据范围」，
 * 余额视图和搜索框都不会挂载。按仓库显式补上本流程要用的范围。
 */
export async function ensureWarehouseStockScope(warehouseCode: string): Promise<void> {
  const token = await apiToken("admin");
  const warehouses = await apiGet<{ items: Array<{ id: string; warehouse_code: string }> }>(
    token,
    "/admin/warehouses",
    { warehouse_code: warehouseCode, page: 1, page_size: 100 },
  );
  const warehouse = warehouses.items.find((row) => row.warehouse_code === warehouseCode);
  if (!warehouse) throw new Error(`缺少库存测试仓库：${warehouseCode}`);
  for (const grant of WAREHOUSE_STOCK_GRANTS) {
    const page = await apiGet<{ items?: StockScopeRow[] }>(token, "/admin/data-scopes", {
      subject_type: "role",
      subject_id: grant.roleId,
      resource: grant.resource,
      page: 1,
      page_size: 100,
    });
    if ((page.items ?? []).some((row) => scopeCovers(row, grant.actions, warehouse.id))) continue;
    await apiPost(token, "/admin/data-scopes", {
      schema_version: 2,
      subject_type: "role",
      subject_id: grant.roleId,
      resource: grant.resource,
      actions: grant.actions,
      target_dimension: "warehouse",
      scope_type: "organization",
      scope_targets: [warehouse.id],
      target_mode: "explicit",
      include_descendants: null,
      enabled: true,
    });
  }
}

/** 只准备零数量仓库/SKU 维度；可用库存必须由浏览器提交盘盈并审批形成。 */
export async function ensureZeroBalanceDimension(
  warehouseCode: string,
  skuNo: string,
): Promise<void> {
  const token = await apiToken("cangchu");
  const warehouses = await apiGet<{ items: Array<{ id: string; warehouse_code: string }> }>(
    token,
    "/admin/warehouses",
    { warehouse_code: warehouseCode, page: 1, page_size: 100 },
  );
  const skus = await apiGet<{ items: Array<{ id: string; sku_no: string }> }>(
    token,
    "/admin/skus",
    { q: skuNo, page: 1, page_size: 100 },
  );
  const warehouse = warehouses.items.find((row) => row.warehouse_code === warehouseCode);
  const sku = skus.items.find((row) => row.sku_no === skuNo);
  if (!warehouse || !sku) throw new Error(`缺少库存测试主数据：${warehouseCode} / ${skuNo}`);
  const config = fileURLToPath(new URL("../../backend/config.toml", import.meta.url));
  const settings = JSON.parse(
    execFileSync(
      "python3",
      [
        "-c",
        "import json,sys,tomllib; print(json.dumps(tomllib.load(open(sys.argv[1], 'rb'))['database']))",
        config,
      ],
      { encoding: "utf8" },
    ),
  ) as { uri: string; db_name: string };
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
