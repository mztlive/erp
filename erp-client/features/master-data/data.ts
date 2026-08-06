/**
 * W14 基础资料：展示用常量与纯函数（不含 seed / mock 数据）。
 */

import type {
  MasterDataListItem,
  MasterDataResource,
} from "@/features/master-data/types"

/** Warehouse write always fail-closed while write ownership unconfirmed. */
export const WAREHOUSE_WRITE_CODE = "WAREHOUSE_WRITE_OWNER_UNCONFIRMED"
export const WAREHOUSE_WRITE_MESSAGE =
  "仓库资料暂不可维护：目前只能查看，不能新建、更新或停用。维护功能尚未开放。"

export function resourceLabel(resource: MasterDataResource): string {
  const found = (
    [
      ["sellable-items", "公司商品池"],
      ["products", "商品与 SKU"],
      ["categories", "商品分类"],
      ["brands", "品牌"],
      ["voucher-categories", "卡券类目"],
      ["suppliers", "供应商与资质"],
      ["warehouses", "仓库"],
    ] as const
  ).find(([k]) => k === resource)
  return found?.[1] ?? resource
}

export function computeMetrics(rows: readonly MasterDataListItem[]) {
  return [
    {
      key: "all",
      label: "全部",
      value: rows.length,
      detail: "当前分类",
    },
    {
      key: "enabled",
      label: "当前启用",
      value: rows.filter((r) => r.lifecycleStatus === "ENABLED").length,
      detail: "启用状态",
    },
    {
      key: "disabled",
      label: "当前停用",
      value: rows.filter((r) => r.lifecycleStatus === "DISABLED").length,
      detail: "历史保留",
    },
    {
      key: "pending",
      label: "待生效更新",
      value: rows.filter((r) => r.revisionTiming === "FUTURE").length,
      detail: "版本状态 · 不是启用状态",
    },
  ] as const
}
