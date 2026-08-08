import { z } from "zod"

import { compareDecimal } from "@/lib/fixed-decimal"
import type {
  ReviseSupplierOfferingInput,
  SafeOfferingDraftView,
  SupplierOfferingRevisionView,
} from "@/features/supplier-catalog/types"

function decimalString(label: string, maxScale: number, positive = false) {
  return z
    .string()
    .trim()
    .regex(
      new RegExp(`^\\d+(?:\\.\\d{1,${maxScale}})?$`),
      `${label}最多保留 ${maxScale} 位小数`
    )
    .refine((value) => !positive || /[1-9]/.test(value), `${label}必须大于 0`)
}

function decimalAtMost(value: string, maximum: string, maxScale: number) {
  try {
    return compareDecimal(value, maximum, maxScale) <= 0
  } catch {
    return false
  }
}

/** 供给条件表单 schema；队列页的"确认建议供给"与列表/详情页的"改供给价"共用同一套字段与校验。 */
export const offeringDraftSchema = z.object({
  dropshipSupplyPriceGross: decimalString("一件代发供给价", 4, true),
  bulkSupplyPriceGross: decimalString("集采供给价", 4, true),
  dropshipExpress: z.string(),
  inputTaxRate: z.string().refine(
    (value) =>
      /^\d+(?:\.\d{1,6})?$/.test(value.trim()) &&
      decimalAtMost(value, "1", 6),
    "进项税率必须为 0 到 1 的十进制数"
  ),
  freightAmount: decimalString("运费", 2),
  serviceFeeAmount: decimalString("服务费", 2),
  minimumOrderQuantity: decimalString("最小起订量", 6, true),
  supplyRegionText: z.string().trim().min(1, "请填写可供区域"),
  productCapabilitiesText: z.string(),
  validFrom: z.string().min(1, "请选择生效日期"),
  validTo: z.string(),
  status: z.enum(["ACTIVE", "PAUSED", "STOPPED"]),
  note: z.string(),
})

export type OfferingDraftValues = z.infer<typeof offeringDraftSchema>

function normalizeStatus(
  status: SupplierOfferingRevisionView["status"] | undefined
): "ACTIVE" | "PAUSED" | "STOPPED" {
  if (status === "PAUSED" || status === "STOPPED") return status
  return "ACTIVE"
}

/** 用现有供给条件的当前修订预填表单，用于"改供给价"这类随时可发起的编辑。 */
export function offeringDefaultsFromCurrentRevision(
  revision: SupplierOfferingRevisionView | undefined
): OfferingDraftValues {
  return {
    dropshipSupplyPriceGross: revision?.dropshipSupplyPriceGross ?? "",
    bulkSupplyPriceGross: revision?.bulkSupplyPriceGross ?? "",
    dropshipExpress: revision?.dropshipExpress ?? "",
    inputTaxRate: revision?.inputTaxRate ?? "",
    freightAmount: revision?.freightAmount ?? "0.00",
    serviceFeeAmount: revision?.serviceFeeAmount ?? "0.00",
    minimumOrderQuantity: revision?.minimumOrderQuantity ?? "",
    supplyRegionText: revision?.supplyRegion.join("、") ?? "",
    productCapabilitiesText: revision?.productCapabilities.join("、") ?? "",
    validFrom: revision?.validFrom ?? "",
    validTo: revision?.validTo ?? "",
    status: normalizeStatus(revision?.status),
    note: "",
  }
}

/** 用供应商来源变化推导出的建议供给条件预填表单，用于队列页确认来源变化。 */
export function offeringDefaultsFromProposed(
  proposed: SafeOfferingDraftView | undefined,
  currentStatus: SupplierOfferingRevisionView["status"] | undefined
): OfferingDraftValues {
  return {
    dropshipSupplyPriceGross: proposed?.dropshipSupplyPriceGross ?? "",
    bulkSupplyPriceGross: proposed?.bulkSupplyPriceGross ?? "",
    dropshipExpress: proposed?.dropshipExpress ?? "",
    inputTaxRate: proposed?.inputTaxRate ?? "",
    freightAmount: proposed?.freightAmount ?? "0.00",
    serviceFeeAmount: proposed?.serviceFeeAmount ?? "0.00",
    minimumOrderQuantity: proposed?.minimumOrderQuantity ?? "",
    supplyRegionText: proposed?.supplyRegion.join("、") ?? "",
    productCapabilitiesText: proposed?.productCapabilities.join("、") ?? "",
    validFrom: proposed?.validFrom ?? "",
    validTo: proposed?.validTo ?? "",
    status: normalizeStatus(currentStatus),
    note: "",
  }
}

/** 把表单值拼成修订请求；offeringId/expectedRevisionNo/idempotencyKey 由调用方按场景提供。 */
export function offeringRevisionPayload(
  value: OfferingDraftValues,
  extra: {
    offeringId: string
    expectedRevisionNo: number
    availableQuantity?: string
    idempotencyKey: string
    defaultChangeReason: string
  }
): ReviseSupplierOfferingInput {
  return {
    offeringId: extra.offeringId,
    expectedRevisionNo: extra.expectedRevisionNo,
    dropshipSupplyPriceGross: value.dropshipSupplyPriceGross,
    bulkSupplyPriceGross: value.bulkSupplyPriceGross,
    dropshipExpress: value.dropshipExpress.trim() || undefined,
    inputTaxRate: value.inputTaxRate.trim(),
    freightAmount: value.freightAmount,
    serviceFeeAmount: value.serviceFeeAmount,
    bulkMinimumOrderQuantity: value.minimumOrderQuantity,
    supplyRegion: value.supplyRegionText
      .split(/[，,]/)
      .map((entry) => entry.trim())
      .filter(Boolean),
    productCapabilities: value.productCapabilitiesText
      .split(/[，,]/)
      .map((entry) => entry.trim())
      .filter(Boolean),
    validFrom: value.validFrom,
    validTo: value.validTo || undefined,
    availableQuantity: extra.availableQuantity,
    status: value.status,
    changeReason: value.note.trim() || extra.defaultChangeReason,
    idempotencyKey: extra.idempotencyKey,
  }
}
