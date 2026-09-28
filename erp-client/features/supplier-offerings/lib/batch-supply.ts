import type { z } from "zod"
import type { FixedSku, SupplierOfferingView } from "../types"
import {
    registerSupplyDefaults,
    registerSupplySchema,
} from "./register-supply-form"
import {
    availabilitySchema,
    idempotencyKey,
    percentageFromRate,
    rateFromPercentage,
    splitValues,
} from "./offering-forms"

export type BatchMode = "create" | "revise" | "availability"
export type RowStatus =
    | "DRAFT"
    | "READY"
    | "INVALID"
    | "SUCCEEDED"
    | "FAILED"
    | "UNKNOWN"
export type SupplyFields = z.input<typeof registerSupplySchema>
export type BatchInput = Record<string, unknown>
export type BatchRow = SupplyFields & {
    rowId: string
    skuCode: string
    skuName: string
    offeringId: string
    expectedRevision: number | null
    expectedVersion: number | null
    capabilities: readonly string[]
    selected: boolean
    status: RowStatus
    message: string
    key: string
    source: "MANUAL" | "EXCEL"
    frozen: BatchInput | null
}
export type BatchValues = {
    supplierId: string
    changeReason: string
    applyAvailabilityStatus: boolean
    applyQuantity: boolean
    common: SupplyFields
    rows: BatchRow[]
    message: string
}
export type BatchResult = {
    rows: {
        row_id: string
        status: RowStatus
        message?: string | null
        result?: unknown
    }[]
}
export const BATCH_LIMIT = 100
export const MODE_LABELS: Record<BatchMode, string> = {
    create: "批量添加供给",
    revise: "批量调价与条款",
    availability: "批量更新可供情况",
}
export const STATUS_LABELS: Record<RowStatus, string> = {
    DRAFT: "待校验",
    READY: "校验通过",
    INVALID: "请修正",
    SUCCEEDED: "已完成",
    FAILED: "未保存",
    UNKNOWN: "待确认结果",
}

export function newBatchRow(sku: FixedSku): BatchRow {
    return {
        ...registerSupplyDefaults(sku.skuId),
        rowId: sku.skuId,
        skuCode: sku.skuCode,
        skuName: sku.skuName,
        offeringId: "",
        expectedRevision: null,
        expectedVersion: null,
        capabilities: [],
        selected: true,
        status: "DRAFT",
        message: "",
        key: idempotencyKey("batch-supply"),
        source: "MANUAL",
        frozen: null,
    }
}
export function offeringBatchRow(item: SupplierOfferingView): BatchRow {
    return {
        ...newBatchRow({
            skuId: item.sku_id,
            skuCode: item.sku_no ?? "",
            skuName: item.sku_name ?? "商品规格",
            specification: item.specification ?? "",
            baseUnit: "",
        }),
        rowId: item.id,
        offeringId: item.id,
        supplierId: item.supplier_id,
        supplierSkuCode: item.supplier_sku_code,
        supplierProductCode: item.supplier_product_code ?? "",
        dropshipPrice: item.dropship_supply_price_gross ?? "",
        bulkPrice: item.bulk_supply_price_gross ?? "",
        inputTaxPercentage: percentageFromRate(item.input_tax_rate),
        minimumQuantity: item.bulk_minimum_order_quantity ?? "",
        supplyRegionText: item.supply_region.join("、"),
        capabilities: item.product_capabilities,
        validFrom: item.valid_from ?? "",
        validTo: item.valid_to ?? "",
        validityMode: item.valid_to ? "dated" : "ongoing",
        dropshipExpress: item.dropship_express ?? "",
        freightAmount: item.freight_amount ?? "",
        serviceFeeAmount: item.service_fee_amount ?? "",
        availableQuantity: item.available_quantity ?? "",
        quantityMode: item.available_quantity == null ? "unknown" : "provided",
        availabilityStatus: item.availability_status ?? "UNAVAILABLE",
        expectedRevision: item.current_revision_no ?? null,
        expectedVersion: item.availability_version ?? null,
    }
}
export function batchDefaults(
    mode: BatchMode,
    skus: readonly FixedSku[],
    offerings: readonly SupplierOfferingView[],
    supplierId: string,
): BatchValues {
    const common = registerSupplyDefaults()
    // 未填写的公共设置不覆盖行值；日期、区域、数量均需用户明确选择。
    common.minimumQuantity = ""
    common.validFrom = ""
    return {
        supplierId,
        changeReason: MODE_LABELS[mode],
        applyAvailabilityStatus: false,
        applyQuantity: false,
        common,
        rows:
            mode === "create"
                ? skus.map(newBatchRow)
                : offerings.map(offeringBatchRow),
        message: "",
    }
}
export const lockedRow = (row: BatchRow) =>
    row.status === "SUCCEEDED" || row.status === "UNKNOWN"
export function rowErrors(
    row: BatchRow,
    values: BatchValues,
    mode: BatchMode,
): string {
    if (lockedRow(row)) return ""
    const value = {
        ...row,
        supplierId: mode === "create" ? values.supplierId : row.supplierId,
        changeReason: values.changeReason,
    }
    const parsed =
        mode === "availability"
            ? availabilitySchema.safeParse(value)
            : registerSupplySchema.safeParse(value)
    const labels: Record<string, string> = {
        skuId: "公司 SKU",
        supplierId: "供应商",
        supplierSkuCode: "供应商订货编码",
        dropshipPrice: "代发含税价",
        bulkPrice: "集采含税价",
        inputTaxPercentage: "税率",
        minimumQuantity: "集采起订量",
        supplyRegionText: "可供区域",
        validFrom: "生效日期",
        validTo: "失效日期",
        availableQuantity: "可供数量",
        changeReason: "变更原因",
        freightAmount: "运费",
        serviceFeeAmount: "服务费",
    }
    const issues = new Map<string, string>()
    if (!parsed.success)
        for (const issue of parsed.error.issues) {
            const field = String(issue.path[0] ?? "")
            if (!issues.has(field))
                issues.set(
                    field,
                    `${labels[field] ?? "本行"}：${issue.message}`,
                )
        }
    const errors = [...issues.values()]
    if (mode === "availability" && row.expectedVersion == null)
        errors.push("缺少可供版本，请刷新列表")
    if (mode === "revise" && row.expectedRevision == null)
        errors.push("缺少条款版本，请刷新列表")
    if (
        mode === "availability" &&
        row.quantityMode === "provided" &&
        !row.availableQuantity.trim()
    )
        errors.push("请填写数量或选择数量未提供")
    if (
        mode === "create" &&
        values.rows.some(
            (other) =>
                other.rowId !== row.rowId &&
                other.selected &&
                other.supplierSkuCode.trim() === row.supplierSkuCode.trim(),
        )
    )
        errors.push("供应商订货编码在本批重复")
    return [...new Set(errors)].join("；")
}
export function rowInput(
    row: BatchRow,
    values: BatchValues,
    mode: BatchMode,
): BatchInput {
    if (row.frozen) return row.frozen
    const base = {
        change_reason: values.changeReason.trim(),
        idempotency_key: row.key,
    }
    const availability = {
        availability_status: row.availabilityStatus,
        available_quantity:
            row.quantityMode === "unknown"
                ? null
                : row.availableQuantity.trim(),
    }
    if (mode === "availability")
        return {
            offering_id: row.offeringId,
            command: {
                ...base,
                ...availability,
                expected_version: row.expectedVersion,
            },
        }
    const terms = {
        dropship_supply_price_gross: row.dropshipPrice.trim(),
        bulk_supply_price_gross: row.bulkPrice.trim(),
        input_tax_rate: rateFromPercentage(row.inputTaxPercentage),
        bulk_minimum_order_quantity: row.minimumQuantity.trim(),
        supply_region: splitValues(row.supplyRegionText),
        product_capabilities: row.capabilities,
        valid_from: row.validFrom,
        valid_to: row.validityMode === "dated" ? row.validTo : null,
        dropship_express: row.dropshipExpress.trim() || null,
        freight_amount: row.freightAmount.trim() || null,
        service_fee_amount: row.serviceFeeAmount.trim() || null,
    }
    if (mode === "revise")
        return {
            offering_id: row.offeringId,
            command: {
                ...base,
                terms,
                expected_revision_no: row.expectedRevision,
            },
        }
    return {
        ...base,
        ...availability,
        terms,
        sku_id: row.skuId,
        supplier_id: values.supplierId,
        supplier_sku_code: row.supplierSkuCode.trim(),
        supplier_product_code: row.supplierProductCode.trim() || null,
        source_type: row.source,
    }
}

export type TextFieldKey =
    | "supplierSkuCode"
    | "dropshipPrice"
    | "bulkPrice"
    | "inputTaxPercentage"
    | "minimumQuantity"
    | "supplyRegionText"
    | "validFrom"
    | "validTo"
    | "availableQuantity"
    | "supplierProductCode"
    | "dropshipExpress"
    | "freightAmount"
    | "serviceFeeAmount"
export const GRID_FIELDS: { key: TextFieldKey; label: string }[] = [
    { key: "supplierSkuCode", label: "供应商订货编码" },
    { key: "dropshipPrice", label: "代发含税价" },
    { key: "bulkPrice", label: "集采含税价" },
    { key: "inputTaxPercentage", label: "税率 %" },
    { key: "minimumQuantity", label: "集采起订量" },
    { key: "supplyRegionText", label: "可供区域" },
]
export const EXTRA_FIELDS: { key: TextFieldKey; label: string }[] = [
    { key: "supplierProductCode", label: "供应商商品编码" },
    { key: "validFrom", label: "生效日期" },
    { key: "validTo", label: "失效日期（留空长期有效）" },
    { key: "dropshipExpress", label: "代发快递" },
    { key: "freightAmount", label: "运费" },
    { key: "serviceFeeAmount", label: "服务费" },
]
