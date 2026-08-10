import { WAREHOUSE_WRITE_CODE, WAREHOUSE_WRITE_MESSAGE } from "@/features/master-data/data"
import type { EnableStatus } from "@/features/master-data/api/contracts"
import type {
  LifecycleStatus,
  MasterDataListItem,
  MasterDataResource,
  ProductKind,
} from "@/features/master-data/types"
import { PRODUCT_KIND_LABELS } from "@/features/master-data/types"
import type { ApiError } from "@/lib/api/errors"

export const LIST_PAGE_SIZE = 100

export const isApiError = (error: unknown): error is ApiError =>
  typeof error === "object" &&
  error !== null &&
  "kind" in error &&
  "message" in error

export const asLifecycle = (status: EnableStatus | string): LifecycleStatus =>
  status === "active" || status === "ACTIVE" || status === "ENABLED"
    ? "ENABLED"
    : "DISABLED"

export const lifecycleLabel = (status: LifecycleStatus): string =>
  status === "ENABLED" ? "当前启用" : "当前停用"

export const lifecycleTone = (
  status: LifecycleStatus
): MasterDataListItem["lifecycleTone"] =>
  status === "ENABLED" ? "success" : "neutral"

/**
 * 生成业务编号（前端未暴露编号录入时的临时唯一码）。
 *
 * 格式：`{prefix}-{timestamp36}{random36}`，避免把幂等键前缀截断后
 * 拼成固定编号（例如 `create-supplier-...` → 永远是 `PTY-createsupp`）。
 */
export function genBusinessCode(prefix: string): string {
  const stamp = Date.now().toString(36).toUpperCase()
  const rand = Math.random().toString(36).slice(2, 8).toUpperCase()
  return `${prefix}-${stamp}${rand}`
}

export const todayDateOnly = (): string => {
  const now = new Date()
  const y = now.getFullYear()
  const m = String(now.getMonth() + 1).padStart(2, "0")
  const d = String(now.getDate()).padStart(2, "0")
  return `${y}-${m}-${d}`
}

export const isoNow = (): string => new Date().toISOString()

export const tsToIso = (seconds: number | undefined): string => {
  if (!seconds) return isoNow()
  return new Date(seconds * 1000).toISOString()
}

export const productKindLabel = (kind: string | undefined): string => {
  if (!kind) return ""
  if (kind in PRODUCT_KIND_LABELS) {
    return PRODUCT_KIND_LABELS[kind as ProductKind]
  }
  // backend OfflineService label
  if (kind === "OFFLINE_SERVICE") return "线下服务"
  return kind
}

export const settlementLabel = (mode: string | undefined): string => {
  switch (mode) {
    case "prepayment":
      return "预付款"
    case "pay_after_use":
      return "先用后付"
    case "cash_settlement":
      return "现结"
    default:
      return mode ?? ""
  }
}

export const invoiceLabel = (type: string | undefined): string => {
  switch (type) {
    case "vat_special":
      return "增值税专用发票"
    case "vat_normal":
      return "增值税普通发票"
    case "electronic":
      return "电子发票"
    default:
      return type ?? ""
  }
}

export const settlementToBackend = (label: string | undefined): string => {
  switch (label) {
    case "预付款":
      return "prepayment"
    case "先用后付":
      return "pay_after_use"
    case "现结":
      return "cash_settlement"
    default:
      return "prepayment"
  }
}

export const invoiceToBackend = (label: string | undefined): string => {
  switch (label) {
    case "增值税专用发票":
      return "vat_special"
    case "增值税普通发票":
      return "vat_normal"
    case "电子发票":
      return "electronic"
    default:
      return "vat_normal"
  }
}

/** 后端 capability_code → 表单多选中文标签。 */
export const capabilityLabel = (code: string | undefined): string => {
  switch (code) {
    case "physical":
      return "实物商品"
    case "virtual":
      return "虚拟商品"
    case "offline_service":
      return "线下服务"
    case "api":
      return "API"
    case "printing":
      return "印刷"
    default:
      return code ?? ""
  }
}

export const capabilityToBackend = (label: string): string | null => {
  switch (label.trim()) {
    case "实物商品":
      return "physical"
    case "虚拟商品":
      return "virtual"
    case "线下服务":
      return "offline_service"
    case "API":
      return "api"
    case "印刷":
      return "printing"
    default:
      return null
  }
}

/** 后端评级代码（A/B/C/D 或已是「A 级」）→ 表单选项。 */
export const ratingLabel = (rating: string | undefined): string => {
  if (!rating) return ""
  const trimmed = rating.trim()
  if (/^[ABCD]$/i.test(trimmed)) return `${trimmed.toUpperCase()} 级`
  if (/^[ABCD]\s*级$/i.test(trimmed)) {
    return `${trimmed.charAt(0).toUpperCase()} 级`
  }
  return trimmed
}

export const ratingToBackend = (label: string | undefined): string => {
  if (!label) return "C"
  const m = label.trim().match(/^([ABCD])/i)
  return m ? m[1].toUpperCase() : "C"
}

/**
 * 经营类目暂无独立后端字段；编码进商务版本 `payment_term_snapshot`
 *（结算方式本身走 `settlement_mode` 枚举，快照仅作展示/回填载体）。
 * 标记串需稳定，加载时原样解析。
 */
export const BUSINESS_CATEGORY_MARK = "｜经营类目："

/** 结算文案 + 经营类目 → 付款条件快照（≤64 字）。 */
export const buildPaymentTermSnapshot = (
  settlement: string | undefined,
  businessCategory: string | undefined
): string => {
  const base = (settlement?.trim() || "默认付款条件").slice(0, 64)
  const cat = businessCategory?.trim()
  if (!cat) return base
  const encoded = `${base}${BUSINESS_CATEGORY_MARK}${cat}`
  return [...encoded].slice(0, 64).join("")
}

/** 从付款条件快照解析经营类目（无标记则空）。 */
export const parseBusinessCategoryFromSnapshot = (
  snapshot: string | null | undefined
): string => {
  if (!snapshot) return ""
  const idx = snapshot.indexOf(BUSINESS_CATEGORY_MARK)
  if (idx < 0) return ""
  return snapshot.slice(idx + BUSINESS_CATEGORY_MARK.length).trim()
}

/** 百分制评分：合法则返回 0–100 整数，否则 undefined。 */
export const parseScore100 = (raw: string | undefined): number | undefined => {
  if (raw == null || !String(raw).trim()) return undefined
  const n = Number.parseInt(String(raw).trim(), 10)
  if (!Number.isFinite(n) || n < 0 || n > 100) return undefined
  return n
}

/** 将用户输入的整数百分数转换为后端 [0, 1) 税率字符串。 */
export const normalizeTaxRate = (raw: string | undefined): string => {
  const text = (raw ?? "").trim().replace(/%$/, "")
  if (!text) return "0.13"
  if (!/^(0|[1-9]\d?)$/.test(text)) return "0.13"
  const value = Number(text)
  return String(value / 100)
}

/** 将后端 [0, 1) 税率转换为页面百分数输入值。 */
export const taxRatePercent = (raw: string | null | undefined): string => {
  if (!raw?.trim()) return ""
  const value = Number(raw)
  if (!Number.isFinite(value) || value < 0 || value >= 1) return ""
  return String(Math.round(value * 100))
}

export const pickDefaultOrFirst = <T extends { is_default?: boolean }>(
  items: readonly T[]
): T | undefined => items.find((item) => item.is_default) ?? items[0]

/** 事实行：空值不写入，避免编辑回填被「—」占位污染。 */
export function fact(
  label: string,
  value: string | number | null | undefined
): { label: string; value: string } | null {
  if (value === null || value === undefined) return null
  const text = String(value).trim()
  if (!text || text === "—") return null
  return { label, value: text }
}

export function factsOf(
  ...rows: Array<{ label: string; value: string } | null>
): Array<{ label: string; value: string }> {
  return rows.filter(
    (row): row is { label: string; value: string } => row !== null
  )
}

export const commonActions = (
  resource: MasterDataResource,
  lifecycle: LifecycleStatus
): Pick<MasterDataListItem, "allowedActions" | "actionBlockers"> => {
  if (resource === "sellable-items") {
    return {
      allowedActions: ["VIEW", "EXPORT_ROW"],
      actionBlockers: [
        {
          action: "CREATE_REVISION",
          code: "SELLABLE_READ_ONLY",
          message: "公司商品池由销售资格实时计算，请在公司商品中维护销售资料。",
        },
        {
          action: "DISABLE",
          code: "SELLABLE_READ_ONLY",
          message: "公司商品池没有独立启停状态，请维护公司 SKU 或供应商供给。",
        },
      ],
    }
  }
  if (resource === "warehouses") {
    return {
      allowedActions: ["VIEW", "EXPORT_ROW"],
      actionBlockers: [
        {
          action: "CREATE",
          code: WAREHOUSE_WRITE_CODE,
          message: WAREHOUSE_WRITE_MESSAGE,
        },
        {
          action: "CREATE_REVISION",
          code: WAREHOUSE_WRITE_CODE,
          message: WAREHOUSE_WRITE_MESSAGE,
        },
        {
          action: "DISABLE",
          code: WAREHOUSE_WRITE_CODE,
          message: WAREHOUSE_WRITE_MESSAGE,
        },
        {
          action: "MAINTAIN_POLICY",
          code: WAREHOUSE_WRITE_CODE,
          message: WAREHOUSE_WRITE_MESSAGE,
        },
      ],
    }
  }
  // 卡券类目：仅新建 + 编辑；不提供查看详情 / 停用。
  if (resource === "voucher-categories") {
    return {
      allowedActions: ["CREATE_REVISION", "EXPORT_ROW"],
      actionBlockers: [
        {
          action: "VIEW",
          code: "VOUCHER_NO_DETAIL",
          message: "卡券类目在列表原地编辑，不提供独立查看。",
        },
        {
          action: "DISABLE",
          code: "VOUCHER_NO_DISABLE",
          message: "卡券类目不支持停用。",
        },
      ],
    }
  }
  // 计量单位：列表 Dialog 更新 / 停用，无侧边预览与独立详情。
  if (resource === "unit-of-measures") {
    const allowed: string[] = ["CREATE_REVISION", "EXPORT_ROW"]
    const blockers: Array<{ action: string; code: string; message: string }> = [
      {
        action: "VIEW",
        code: "UNIT_NO_SIDE_PREVIEW",
        message: "计量单位在列表 Dialog 维护，不提供侧边预览。",
      },
    ]
    if (lifecycle === "ENABLED") {
      allowed.push("DISABLE")
    } else {
      blockers.push({
        action: "DISABLE",
        code: "ALREADY_DISABLED",
        message: "资料已停用；不是删除，历史记录仍可查看。",
      })
    }
    return { allowedActions: allowed, actionBlockers: blockers }
  }
  const allowed: string[] = ["VIEW", "EXPORT_ROW"]
  const blockers: Array<{ action: string; code: string; message: string }> = []
  if (lifecycle === "ENABLED") {
    allowed.push("CREATE_REVISION", "DISABLE")
  } else {
    allowed.push("CREATE_REVISION")
    blockers.push({
      action: "DISABLE",
      code: "ALREADY_DISABLED",
      message: "资料已停用；不是删除，历史记录仍可查看。",
    })
  }
  return { allowedActions: allowed, actionBlockers: blockers }
}
