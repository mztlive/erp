import { execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { fileURLToPath } from "node:url";
import { apiGet, apiToken } from "./api";
import { ensurePersonWarehouseScope } from "./person-data-scope";

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
  const config = process.env.ERP_E2E_CONFIG_PATH ??
    fileURLToPath(new URL("../../backend/config.toml", import.meta.url));
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
