import { z } from "zod"
import { getErrorMessage } from "@/lib/api/errors"
import type {
    NewProductInput,
    PortalBatchMode,
    PortalBatchPhase,
    PortalBatchResult,
    PortalBatchRow,
    PortalCatalogSku,
    PortalOffering,
} from "../types"
import {
    optionalQuantity,
    termsDefaults,
    termsFromValues,
    termsSchema,
} from "./forms"
import { commandKey, offeringTerms } from "./presentation"

export const PORTAL_BATCH_LIMIT = 100
export const PORTAL_BATCH_BYTES = 5 * 1024 * 1024
export const batchFields = [
    "skuNo",
    "groupCode",
    "productName",
    "productKind",
    "brandRaw",
    "categoryRaw",
    "model",
    "description",
    "skuName",
    "specs",
    "unitRaw",
    "barcode",
    "orderingCode",
    "dropshipPrice",
    "bulkPrice",
    "taxPercentage",
    "minimumQuantity",
    "regions",
    "validFrom",
    "validTo",
    "express",
    "freight",
    "serviceFee",
    "quantity",
    "availability",
    "reason",
] as const
export type BatchField = (typeof batchFields)[number]
export type BatchValues = Record<BatchField, string>
export type BatchColumn = { key: BatchField; label: string }
export type BatchStatus =
    | "EDITING"
    | "READY"
    | "INVALID"
    | "SUCCEEDED"
    | "FAILED"
    | "UNKNOWN"
    | "PREPARED"
export type BatchEditorRow = {
    rowId: string
    key: string
    values: BatchValues
    status: BatchStatus
    message: string
    fieldErrors: Record<string, string>
    reportedAt: number | string
    matchedName?: string
    applicationId?: string
    applicationVersion?: number
    commandPhase?: PortalBatchPhase
    prepareCommand?: PortalBatchRow
    command?: PortalBatchRow
}
export type PreparedBatch = {
    rows: BatchEditorRow[]
    commands: PortalBatchRow[]
}
export const batchModeLabels: Record<PortalBatchMode, string> = {
    quote: "已有商品批量报价",
    terms: "已有供给批量调价",
    new_product: "新品批量提报",
    availability: "批量更新可供",
}
export const batchStatusLabels: Record<BatchStatus, string> = {
    EDITING: "待校验",
    READY: "校验通过",
    INVALID: "需要修正",
    SUCCEEDED: "已完成",
    FAILED: "未完成",
    UNKNOWN: "待确认结果",
    PREPARED: "待补图提交",
}
const column = (key: BatchField, label: string): BatchColumn => ({ key, label })
const identityColumns = [
    column("skuNo", "公司SKU编号"),
    column("orderingCode", "供应商订货编码"),
]
const termsColumns = [
    column("dropshipPrice", "代发含税供货价"),
    column("bulkPrice", "集采含税供货价"),
    column("taxPercentage", "税率（%）"),
    column("minimumQuantity", "集采起订量"),
    column("regions", "可供区域"),
    column("validFrom", "生效日期"),
    column("validTo", "失效日期"),
    column("express", "代发快递说明"),
    column("freight", "运费"),
    column("serviceFee", "服务费"),
]
const availabilityColumns = [
    column("availability", "可供状态"),
    column("quantity", "可供数量"),
]
const productColumns = [
    column("groupCode", "商品组编号"),
    column("productName", "商品名称"),
    column("productKind", "商品类型"),
    column("brandRaw", "原始品牌"),
    column("categoryRaw", "原始分类完整路径"),
    column("model", "型号"),
    column("description", "商品说明"),
    column("skuName", "SKU名称"),
    column("specs", "规格属性"),
    column("unitRaw", "原始单位及包装含义"),
    column("barcode", "条码"),
    column("orderingCode", "供应商订货编码"),
]
export const productSharedFields: BatchField[] = [
    "productName",
    "productKind",
    "brandRaw",
    "categoryRaw",
    "model",
    "description",
]
export function batchColumns(mode: PortalBatchMode): BatchColumn[] {
    if (mode === "availability")
        return [
            ...identityColumns,
            ...availabilityColumns,
            column("reason", "报送说明"),
        ]
    if (mode === "new_product")
        return [
            ...productColumns,
            ...termsColumns,
            column("quantity", "可供数量"),
        ]
    return [
        ...identityColumns,
        ...termsColumns,
        ...(mode === "quote" ? availabilityColumns : []),
        column("reason", "申请原因"),
    ]
}
export function mainBatchColumns(mode: PortalBatchMode): BatchColumn[] {
    const keys: BatchField[] =
        mode === "new_product"
            ? [
                  "groupCode",
                  "productName",
                  "skuName",
                  "orderingCode",
                  "dropshipPrice",
                  "bulkPrice",
              ]
            : mode === "availability"
              ? ["skuNo", "orderingCode", "availability", "quantity", "reason"]
              : [
                    "skuNo",
                    "orderingCode",
                    "dropshipPrice",
                    "bulkPrice",
                    "taxPercentage",
                ]
    return batchColumns(mode).filter((item) => keys.includes(item.key))
}
export function newBatchRow(values: Partial<BatchValues> = {}): BatchEditorRow {
    return {
        rowId: crypto.randomUUID(),
        key: commandKey("batch"),
        values: {
            ...Object.fromEntries(batchFields.map((field) => [field, ""])),
            availability: "有货",
            productKind: "实物",
            ...values,
        } as BatchValues,
        status: "EDITING",
        message: "",
        fieldErrors: {},
        reportedAt: Math.floor(Date.now() / 1000),
    }
}
export const isBatchRowLocked = (row: BatchEditorRow) =>
    !!row.applicationId ||
    ["SUCCEEDED", "UNKNOWN", "PREPARED"].includes(row.status)
export function editBatchRow(
    row: BatchEditorRow,
    values: BatchValues,
): BatchEditorRow {
    if (isBatchRowLocked(row)) throw new Error("已完成或待确认的资料不能修改")
    return {
        ...row,
        values,
        key: commandKey("batch"),
        status: "EDITING",
        message: "",
        fieldErrors: {},
        command: undefined,
        matchedName: undefined,
    }
}
export function editBatchValues(
    rows: BatchEditorRow[],
    updates: Map<string, Partial<BatchValues>>,
    mode: PortalBatchMode,
): BatchEditorRow[] {
    const affectedGroups = new Set<string>()
    for (const row of rows)
        if (updates.has(row.rowId)) {
            affectedGroups.add(row.values.groupCode.trim())
            affectedGroups.add(
                updates.get(row.rowId)?.groupCode?.trim() ??
                    row.values.groupCode.trim(),
            )
        }
    return rows.map((row) => {
        const update = updates.get(row.rowId)
        if (update) return editBatchRow(row, { ...row.values, ...update })
        return mode === "new_product" &&
            affectedGroups.has(row.values.groupCode.trim()) &&
            !isBatchRowLocked(row)
            ? editBatchRow(row, row.values)
            : row
    })
}
export function parseBatchSpecs(value: string) {
    return value
        .split(/[；;\n]/)
        .filter((item) => item.trim())
        .map((item) => {
            const position = item.indexOf("=")
            if (position < 1 || !item.slice(position + 1).trim())
                throw new Error(`规格「${item}」请按“规格名=取值”填写`)
            return {
                attribute_code: item.slice(0, position).trim(),
                attribute_value_code: item.slice(position + 1).trim(),
            }
        })
}
const productKindMap: Record<string, string> = {
    实物: "PHYSICAL",
    虚拟: "VIRTUAL",
    线下服务: "OFFLINE_SERVICE",
    卡券: "VOUCHER",
    PHYSICAL: "PHYSICAL",
    VIRTUAL: "VIRTUAL",
    OFFLINE_SERVICE: "OFFLINE_SERVICE",
    VOUCHER: "VOUCHER",
}
const availabilityMap: Record<string, string> = {
    有货: "AVAILABLE",
    可供: "AVAILABLE",
    临时缺货: "OUT_OF_STOCK",
    不可供: "OUT_OF_STOCK",
    AVAILABLE: "AVAILABLE",
    OUT_OF_STOCK: "OUT_OF_STOCK",
}
const dateValid = (value: string) =>
    /^\d{4}-\d{2}-\d{2}$/.test(value) &&
    !Number.isNaN(new Date(`${value}T00:00:00Z`).getTime()) &&
    new Date(`${value}T00:00:00Z`).toISOString().slice(0, 10) === value
/** 时刻按协议发送Unix秒；已有整数保持原值，日期文本在预检阶段转换。 */
export function batchReportTime(value: number | string): number {
    if (typeof value === "number") {
        if (!Number.isSafeInteger(value))
            throw new Error("实际报送时间无效，请重新核对")
        return value
    }
    const source = value.trim()
    if (
        !source ||
        (/^\d{4}-\d{2}-\d{2}/.test(source) && !dateValid(source.slice(0, 10)))
    )
        throw new Error("实际报送时间无效，请填写有效日期和时间")
    const parsed = Date.parse(source)
    if (!Number.isFinite(parsed))
        throw new Error("实际报送时间无效，请填写有效日期和时间")
    return Math.floor(parsed / 1000)
}
export function hasLegacyBatchReport(
    command: PortalBatchRow | undefined,
    mode: PortalBatchMode,
): boolean {
    if (!command) return false
    if (mode === "quote") {
        const snapshot = command.input.snapshot
        return (
            !!snapshot &&
            typeof snapshot === "object" &&
            "availability_reported_at" in snapshot &&
            (typeof snapshot.availability_reported_at !== "number" ||
                !Number.isSafeInteger(snapshot.availability_reported_at))
        )
    }
    if (mode === "new_product") {
        const input = command.input.input
        return (
            !!input &&
            typeof input === "object" &&
            "skus" in input &&
            Array.isArray(input.skus) &&
            input.skus.some(
                (sku) =>
                    typeof sku.reported_at !== "number" ||
                    !Number.isSafeInteger(sku.reported_at),
            )
        )
    }
    return false
}
function localErrors(
    row: BatchEditorRow,
    mode: PortalBatchMode,
): Record<string, string> {
    const value = row.values
    const errors: Record<string, string> = {}
    if (mode === "quote" || mode === "new_product") {
        try {
            batchReportTime(row.reportedAt)
        } catch {
            errors.reportedAt = "实际报送时间无效，请重新核对后填写"
        }
    }
    if (mode !== "new_product" && !value.skuNo.trim())
        errors.skuNo = "请填写公司SKU编号"
    if (!value.orderingCode.trim())
        errors.orderingCode = "请填写供应商使用的订货编码"
    if (mode === "availability") {
        if (!availabilityMap[value.availability])
            errors.availability = "仅可填写有货或临时缺货"
        if (!optionalQuantity.safeParse(value.quantity).success)
            errors.quantity = "请输入非负数量，最多6位小数"
        if (!value.reason.trim()) errors.reason = "请填写报送说明"
    } else {
        const parsed = termsSchema.safeParse({
            ...termsDefaults,
            ...value,
            availability:
                mode === "quote"
                    ? (availabilityMap[value.availability] ??
                      value.availability)
                    : "AVAILABLE",
            reason: mode === "new_product" ? "新品提报" : value.reason,
        })
        if (!parsed.success)
            for (const issue of parsed.error.issues)
                errors[String(issue.path[0])] ??= issue.message
        if (!dateValid(value.validFrom))
            errors.validFrom = "生效日期请按有效的YYYY-MM-DD填写"
        if (
            value.validTo &&
            (!dateValid(value.validTo) || value.validTo < value.validFrom)
        )
            errors.validTo = "失效日期须有效且不早于生效日期"
    }
    if (mode === "new_product") {
        for (const field of [
            "groupCode",
            "productName",
            "brandRaw",
            "categoryRaw",
            "skuName",
            "unitRaw",
        ] as const)
            if (!value[field].trim()) errors[field] = "此项必须填写原始资料"
        if (!productKindMap[value.productKind])
            errors.productKind = "请填写实物、虚拟、线下服务或卡券"
        try {
            parseBatchSpecs(value.specs)
        } catch (cause) {
            errors.specs =
                cause instanceof Error ? cause.message : "请核对规格属性"
        }
    }
    return errors
}
function errorsMessage(errors: Record<string, string>, mode: PortalBatchMode) {
    const columns = batchColumns(mode)
    return Object.entries(errors)
        .map(
            ([key, message]) =>
                `${columns.find((item) => item.key === key)?.label ?? "资料"}：${message}`,
        )
        .join("；")
}
function sharedProductRows(rows: BatchEditorRow[]) {
    const groups = new Map<string, BatchEditorRow[]>()
    for (const row of rows) {
        const key = row.values.groupCode.trim()
        groups.set(key, [...(groups.get(key) ?? []), row])
    }
    return groups
}
export function validateBatchRows(
    rows: BatchEditorRow[],
    mode: PortalBatchMode,
): BatchEditorRow[] {
    if (!rows.length || rows.length > PORTAL_BATCH_LIMIT)
        throw new Error("每批须包含1至100条SKU或供给数据")
    const editable = rows.filter((row) => !isBatchRowLocked(row))
    const errors = new Map(
        editable.map((row) => [row.rowId, localErrors(row, mode)]),
    )
    const identities = new Map<string, BatchEditorRow[]>()
    for (const row of rows) {
        const identity =
            mode === "new_product"
                ? row.values.orderingCode.trim()
                : `${row.values.skuNo.trim()}\u0000${mode === "quote" ? "" : row.values.orderingCode.trim()}`
        identities.set(identity, [...(identities.get(identity) ?? []), row])
    }
    for (const duplicate of identities.values())
        if (duplicate.length > 1)
            for (const row of duplicate) {
                const error = errors.get(row.rowId)
                if (error)
                    error.orderingCode =
                        "同一批中订货编码或供给目标重复，请核对后移除重复行"
            }
    if (mode === "new_product")
        for (const group of sharedProductRows(rows).values()) {
            const first = group[0]
            for (const row of group)
                for (const field of productSharedFields)
                    if (
                        row.values[field].trim() !== first.values[field].trim()
                    ) {
                        const error = errors.get(row.rowId)
                        if (error)
                            error[field] =
                                "同一商品组的公共资料必须一致，请核对所有规格行"
                    }
            if (
                group.some(isBatchRowLocked) &&
                group.some((row) => !isBatchRowLocked(row))
            )
                for (const row of group) {
                    const error = errors.get(row.rowId)
                    if (error)
                        error.groupCode =
                            "此商品组已有保存记录，请在原草稿继续维护；新商品请使用新的商品组编号"
                }
        }
    return rows.map((row) => {
        const fieldErrors = errors.get(row.rowId)
        const reportedAt =
            fieldErrors &&
            !fieldErrors.reportedAt &&
            (mode === "quote" || mode === "new_product")
                ? batchReportTime(row.reportedAt)
                : row.reportedAt
        const staleCommand =
            fieldErrors && hasLegacyBatchReport(row.command, mode)
        return fieldErrors
            ? ({
                  ...row,
                  reportedAt,
                  command: staleCommand ? undefined : row.command,
                  key: staleCommand ? commandKey("batch") : row.key,
                  fieldErrors,
                  status: Object.keys(fieldErrors).length
                      ? "INVALID"
                      : "EDITING",
                  message: errorsMessage(fieldErrors, mode),
              } as BatchEditorRow)
            : row
    })
}
export type BatchTarget = PortalCatalogSku | PortalOffering
function quantityFits(value: string, precision: number) {
    return (
        !value ||
        !value.includes(".") ||
        value.split(".")[1].replace(/0+$/, "").length <= precision
    )
}
function newProductCommand(group: BatchEditorRow[]): PortalBatchRow {
    const first = group[0]
    const value = first.values
    const input: NewProductInput = {
        name: value.productName.trim(),
        product_kind: productKindMap[value.productKind],
        brand: { raw_name: value.brandRaw.trim() },
        category: { raw_name: value.categoryRaw.trim() },
        model: value.model.trim() || undefined,
        description: value.description.trim() || undefined,
        image_asset_ids: [],
        file_asset_ids: [],
        skus: group.map((row) => ({
            row_id: row.rowId,
            name: row.values.skuName.trim(),
            spec_entries: parseBatchSpecs(row.values.specs),
            unit: { raw_name: row.values.unitRaw.trim() },
            barcode: row.values.barcode.trim() || undefined,
            ordering_code: row.values.orderingCode.trim(),
            supply_terms: termsFromValues(row.values),
            available_quantity: row.values.quantity.trim() || null,
            reported_at: batchReportTime(row.reportedAt),
        })),
    }
    return { row_id: first.rowId, idempotency_key: first.key, input: { input } }
}
export function prepareBatchRows(
    rows: BatchEditorRow[],
    mode: PortalBatchMode,
    targets: Map<string, BatchTarget>,
): PreparedBatch {
    const checked = validateBatchRows(rows, mode)
    if (checked.some((row) => row.status === "INVALID"))
        return { rows: checked, commands: [] }
    const commands: PortalBatchRow[] = []
    const mapped = new Map<string, { command: PortalBatchRow; name?: string }>()
    if (mode === "new_product")
        for (const group of sharedProductRows(
            checked.filter((row) => !isBatchRowLocked(row)),
        ).values()) {
            const command = group[0].command ?? newProductCommand(group)
            commands.push(command)
            for (const row of group) mapped.set(row.rowId, { command })
        }
    else
        for (const row of checked.filter((item) => !isBatchRowLocked(item))) {
            if (row.command) {
                commands.push(row.command)
                mapped.set(row.rowId, {
                    command: row.command,
                    name: row.matchedName,
                })
                continue
            }
            const value = row.values
            const target = targets.get(row.rowId)
            if (!target) {
                row.status = "INVALID"
                row.message =
                    mode === "quote"
                        ? "未在开放目录中精确匹配此SKU，请核对编号或联系采购开放目录"
                        : "未精确匹配自己的供给，请同时核对SKU编号和订货编码"
                continue
            }
            if (
                mode === "quote" &&
                (("own_offering_id" in target && target.own_offering_id) ||
                    ("own_offering" in target && target.own_offering) ||
                    ("offering_id" in target && target.offering_id))
            ) {
                row.status = "INVALID"
                row.message = "此SKU已有自己的供给，请进入已有供给批量调价"
                continue
            }
            if (
                "source_type" in target &&
                (target.source_type === "API" || target.writable === false)
            ) {
                row.status = "INVALID"
                row.message = "此供给由外部系统维护，请联系采购核对"
                continue
            }
            if (
                "unit_precision" in target &&
                (!quantityFits(value.minimumQuantity, target.unit_precision) ||
                    !quantityFits(value.quantity, target.unit_precision))
            ) {
                row.status = "INVALID"
                row.message = `数量须符合${target.unit_name}的精度要求（最多${target.unit_precision}位小数）`
                continue
            }
            const terms =
                mode !== "availability"
                    ? {
                          ...termsFromValues(value),
                          product_capabilities:
                              "source_type" in target
                                  ? offeringTerms(target).product_capabilities
                                  : [],
                      }
                    : undefined
            let input: Record<string, unknown>
            if (mode === "availability") {
                const offering = target as PortalOffering
                input = {
                    offering_id: offering.id,
                    expected_version: offering.availability_version,
                    availability_status: availabilityMap[value.availability],
                    available_quantity: value.quantity.trim() || null,
                    reason: value.reason.trim(),
                }
            } else {
                const snapshot =
                    mode === "quote"
                        ? {
                              kind: "EXISTING_QUOTE",
                              sku_id: target.id,
                              target_version: (target as PortalCatalogSku)
                                  .target_version,
                              supplier_sku_code: value.orderingCode.trim(),
                              supplier_product_code: null,
                              terms,
                              availability_status:
                                  availabilityMap[value.availability],
                              available_quantity: value.quantity.trim() || null,
                              availability_reported_at: batchReportTime(
                                  row.reportedAt,
                              ),
                          }
                        : {
                              kind: "TERMS_CHANGE",
                              offering_id: target.id,
                              expected_offering_version: (
                                  target as PortalOffering
                              ).version,
                              expected_revision_no: (target as PortalOffering)
                                  .current_revision_no,
                              terms,
                          }
                input = { snapshot, reason: value.reason.trim() }
            }
            const command = {
                row_id: row.rowId,
                idempotency_key: row.key,
                input,
            }
            commands.push(command)
            mapped.set(row.rowId, {
                command,
                name:
                    target.name ??
                    ("sku_name" in target ? target.sku_name : undefined),
            })
        }
    return {
        commands: checked.some((row) => row.status === "INVALID")
            ? []
            : commands,
        rows: checked.map((row) => ({
            ...row,
            command: mapped.get(row.rowId)?.command ?? row.command,
            matchedName: mapped.get(row.rowId)?.name ?? row.matchedName,
        })),
    }
}
export function applyBatchResult(
    rows: BatchEditorRow[],
    result: PortalBatchResult,
    mode: PortalBatchMode,
    recoveryOnly = false,
    phase: PortalBatchPhase = "prepare",
): BatchEditorRow[] {
    const results = new Map(result.rows.map((row) => [row.row_id, row]))
    const statusMap: Record<string, BatchStatus> = {
        valid: "READY",
        validation_failed: "INVALID",
        succeeded: "SUCCEEDED",
        replayed: "SUCCEEDED",
        failed: "FAILED",
        unknown: "UNKNOWN",
        READY: "READY",
        INVALID: "INVALID",
        SUCCEEDED: "SUCCEEDED",
        FAILED: "FAILED",
        UNKNOWN: "UNKNOWN",
    }
    return rows.map((row) => {
        if (row.status === "SUCCEEDED" || !row.command) return row
        const item = results.get(row.command.row_id)
        if (!item) return row
        const savedDraft =
            mode === "new_product" &&
            item.result &&
            typeof item.result === "object" &&
            "status" in item.result &&
            typeof item.result.status === "string" &&
            item.result.status.toUpperCase() === "DRAFT" &&
            "id" in item.result &&
            typeof item.result.id === "string"
        const status: BatchStatus = savedDraft
            ? "PREPARED"
            : item.status === "valid" && item.result != null
              ? "SUCCEEDED"
              : (statusMap[item.status] ?? "UNKNOWN")
        const effectiveStatus = status
        const message =
            effectiveStatus === "PREPARED"
                ? "新品草稿已准备，请补充图片后在本批次统一提交"
                : effectiveStatus === "SUCCEEDED"
                  ? mode === "availability"
                      ? "可供情况已更新"
                      : mode === "new_product"
                        ? "原新品申请已提交，待采购确认"
                        : "申请已创建，待采购确认"
                  : recoveryOnly && status === "UNKNOWN"
                    ? hasLegacyBatchReport(row.command, mode)
                        ? "原报送时间需要重新核对，请联系采购确认原操作结果；保留原内容及记录，勿重复提交"
                        : "原提交仍无法确认，请保留原内容和记录，稍后再次确认或联系采购核对"
                    : ((item.message ||
                      ("error" in item && typeof item.error === "string")
                          ? getErrorMessage(
                                item.message || item.error,
                                "请核对本行资料后重试",
                            )
                          : undefined) ??
                      (effectiveStatus === "READY"
                          ? mode === "new_product" && phase === "prepare"
                              ? "校验通过；准备草稿时将再次核对当前资料"
                              : "校验通过；提交时将再次核对当前资料"
                          : effectiveStatus === "UNKNOWN"
                            ? "暂时无法确认，请确认待定结果后继续"
                            : "请核对本行资料后重试"))
        return {
            ...row,
            status: effectiveStatus,
            message,
            applicationId:
                ["SUCCEEDED", "PREPARED"].includes(effectiveStatus) &&
                mode !== "availability" &&
                item.result &&
                typeof item.result === "object" &&
                "id" in item.result &&
                typeof item.result.id === "string"
                    ? item.result.id
                    : row.applicationId,
            applicationVersion:
                item.result &&
                typeof item.result === "object" &&
                "version" in item.result &&
                typeof item.result.version === "number"
                    ? item.result.version
                    : row.applicationVersion,
            commandPhase: phase,
            prepareCommand:
                effectiveStatus === "PREPARED"
                    ? row.command
                    : row.prepareCommand,
            command: effectiveStatus === "PREPARED" ? undefined : row.command,
            fieldErrors: item.field_errors ?? {},
        }
    })
}
export function markBatchUnknown(
    rows: BatchEditorRow[],
    commands: PortalBatchRow[],
): BatchEditorRow[] {
    const ids = new Set(commands.map((command) => command.row_id))
    return rows.map((row) =>
        row.command &&
        ids.has(row.command.row_id) &&
        !["SUCCEEDED", "PREPARED"].includes(row.status)
            ? {
                  ...row,
                  status: "UNKNOWN",
                  message: "处理结果暂未确认，请使用“确认待定结果”继续核对",
              }
            : row,
    )
}
export function batchProcessingCount(
    rows: BatchEditorRow[],
    mode: PortalBatchMode,
): number {
    const pending = rows.filter((row) => row.status !== "SUCCEEDED")
    return mode === "new_product"
        ? new Set(pending.map((row) => row.values.groupCode.trim())).size
        : pending.length
}
export const batchFormSchema = z.object({
    paste: z.string(),
    rows: z
        .array(z.custom<BatchEditorRow>())
        .min(1, "请先加入数据")
        .max(PORTAL_BATCH_LIMIT, "每批最多100行"),
    notice: z.string(),
    hydrated: z.boolean(),
    recoveryBlocked: z.boolean(),
})
