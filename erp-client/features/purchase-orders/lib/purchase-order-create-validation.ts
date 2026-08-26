import { z } from "zod"

import {
    findSourcingOption,
    sourcingQuantityError,
    type SourcingLineInput,
    type SourcingSalesOrder,
} from "@/features/purchase-orders/lib/purchase-order-create-model"
import { compareDecimal, sumFixed } from "@/lib/fixed-decimal"

export type SourcingFormValues = {
    salesOrderId: string
    lines: SourcingLineInput[]
}

/** TanStack Form 表单级校验返回值：按字段名挂错误，字段才能展示。 */
export type SourcingFormValidationError = {
    fields: Record<string, string>
}

const textField = z
    .union([z.string(), z.null(), z.undefined()])
    .transform((value) => (typeof value === "string" ? value.trim() : ""))

const flagField = z
    .union([z.boolean(), z.null(), z.undefined()])
    .transform((value) => value === true)

/**
 * 按当前选源销售单构造建单表单校验。
 *
 * @param order 当前选中的选源销售单；未选时只要求选择销售单。
 * @returns 可挂到 TanStack Form 的 Zod schema。
 */
export function buildSourcingFormSchema(order?: SourcingSalesOrder) {
    const maximumByBasisId = new Map(
        order?.lines.flatMap((line) =>
            line.options.map((option) => [
                `${line.salesOrderLineId}:${option.basisId}`,
                option.maxCreateQuantity,
            ]),
        ) ?? [],
    )
    return z
        .object({
            salesOrderId: textField.pipe(z.string().min(1, "请选择来源销售单")),
            lines: z.array(
                z.object({
                    rowKey: textField,
                    salesOrderLineId: textField,
                    selected: flagField,
                    quantity: textField,
                    basisId: textField,
                    targetWarehouseId: textField.optional().default(""),
                    targetWarehouseName: textField.optional().default(""),
                    expectedDeliveryDate: textField,
                }),
            ),
        })
        .superRefine((value, context) => {
            if (!order) return
            const selected = value.lines.filter((line) => line.selected)
            if (selected.length === 0) {
                context.addIssue({
                    code: "custom",
                    path: ["lines"],
                    message: "请至少选择一条本次供给分配明细",
                })
                return
            }
            const seenAllocations = new Set<string>()
            value.lines.forEach((line, index) => {
                if (!line.selected) return
                const product = order.lines.find(
                    (candidate) =>
                        candidate.salesOrderLineId === line.salesOrderLineId,
                )
                const itemLabel = product?.itemName
                if (!line.basisId) {
                    context.addIssue({
                        code: "custom",
                        path: ["lines", index, "basisId"],
                        message: itemLabel
                            ? `${itemLabel}：请选择履约方案`
                            : "请选择履约方案",
                    })
                    return
                }
                const allocationKey = `${line.salesOrderLineId}:${line.basisId}`
                if (seenAllocations.has(allocationKey)) {
                    context.addIssue({
                        code: "custom",
                        path: ["lines", index, "basisId"],
                        message: itemLabel
                            ? `${itemLabel}：同一履约方案不能重复选择`
                            : "同一履约方案不能重复选择",
                    })
                    return
                }
                seenAllocations.add(allocationKey)
                const option = findSourcingOption(product, line.basisId)
                if (!option) {
                    context.addIssue({
                        code: "custom",
                        path: ["lines", index, "basisId"],
                        message: itemLabel
                            ? `${itemLabel}：该履约方案已失效`
                            : "该履约方案已失效",
                    })
                    return
                }
                const requiresTargetWarehouse =
                    option.sourceType === "PURCHASE" &&
                    option.fulfillmentResponsibility === "WAREHOUSE"
                if (requiresTargetWarehouse && !line.targetWarehouseId) {
                    context.addIssue({
                        code: "custom",
                        path: ["lines", index, "targetWarehouseId"],
                        message: itemLabel
                            ? `${itemLabel}：请选择采购入库目标仓`
                            : "请选择采购入库目标仓",
                    })
                } else if (!requiresTargetWarehouse && line.targetWarehouseId) {
                    context.addIssue({
                        code: "custom",
                        path: ["lines", index, "targetWarehouseId"],
                        message: itemLabel
                            ? `${itemLabel}：当前履约方式不应指定采购入库仓`
                            : "当前履约方式不应指定采购入库仓",
                    })
                }
                const maximum =
                    maximumByBasisId.get(
                        `${line.salesOrderLineId}:${line.basisId}`,
                    ) ?? "0"
                const quantityMessage = sourcingQuantityError(
                    line.quantity,
                    maximum,
                )
                if (quantityMessage) {
                    context.addIssue({
                        code: "custom",
                        path: ["lines", index, "quantity"],
                        message: itemLabel
                            ? `${itemLabel}：${quantityMessage}`
                            : quantityMessage,
                    })
                }
                if (!isBusinessDate(line.expectedDeliveryDate)) {
                    context.addIssue({
                        code: "custom",
                        path: ["lines", index, "expectedDeliveryDate"],
                        message: itemLabel
                            ? `${itemLabel}：请选择预计交付日`
                            : "请选择预计交付日",
                    })
                } else if (
                    product?.deliveryDeadline &&
                    line.expectedDeliveryDate > product.deliveryDeadline
                ) {
                    context.addIssue({
                        code: "custom",
                        path: ["lines", index, "expectedDeliveryDate"],
                        message: `${itemLabel ?? "该明细"}：预计交付日不能晚于销售承诺期限 ${product.deliveryDeadline}`,
                    })
                }
            })
            for (const product of order.lines) {
                const indexes = value.lines
                    .map((line, index) => ({ line, index }))
                    .filter(
                        ({ line }) =>
                            line.selected &&
                            line.salesOrderLineId === product.salesOrderLineId,
                    )
                if (indexes.length === 0) continue
                try {
                    const total = sumFixed(
                        indexes.map(({ line }) => line.quantity),
                        { maxScale: 6, outputScale: 6 },
                    )
                    if (
                        compareDecimal(total, product.remainingQuantity, 6) > 0
                    ) {
                        context.addIssue({
                            code: "custom",
                            path: ["lines", indexes.at(-1)!.index, "quantity"],
                            message: `${product.itemName}：拆分数量合计不能超过 ${product.remainingQuantity}`,
                        })
                    }
                } catch {
                    // 单行数值错误已在上方给出精确提示。
                }
            }
            const stockByBasis = new Map<
                string,
                {
                    quantities: string[]
                    maximum: string
                    warehouseName: string
                    lastIndex: number
                }
            >()
            value.lines.forEach((line, index) => {
                if (!line.selected || !line.basisId) return
                const product = order.lines.find(
                    (candidate) =>
                        candidate.salesOrderLineId === line.salesOrderLineId,
                )
                const option = findSourcingOption(product, line.basisId)
                if (!option || option.sourceType !== "EXISTING_STOCK") return
                const current = stockByBasis.get(option.basisId)
                if (current) {
                    current.quantities.push(line.quantity)
                    current.lastIndex = index
                    return
                }
                stockByBasis.set(option.basisId, {
                    quantities: [line.quantity],
                    maximum:
                        option.sourceAvailableQuantity ??
                        option.maxCreateQuantity,
                    warehouseName: option.warehouseName ?? option.supplierName,
                    lastIndex: index,
                })
            })
            for (const stock of stockByBasis.values()) {
                try {
                    const total = sumFixed(stock.quantities, {
                        maxScale: 6,
                        outputScale: 6,
                    })
                    if (compareDecimal(total, stock.maximum, 6) > 0) {
                        context.addIssue({
                            code: "custom",
                            path: ["lines", stock.lastIndex, "quantity"],
                            message: `${stock.warehouseName}：库存分配合计不能超过 ${stock.maximum}`,
                        })
                    }
                } catch {
                    // 单行数值错误已在上方给出精确提示。
                }
            }
        })
}

/**
 * 把 Zod 校验结果转成 TanStack Form 能挂到字段上的错误。
 *
 * 直接返回 `ZodError` 只会停在表单级，表格里看不到行内提示。
 *
 * @param order 当前选源销售单。
 * @param value 表单值。
 * @returns 字段错误；通过时为空。
 */
export function sourcingFormValidationError(
    order: SourcingSalesOrder | undefined,
    value: SourcingFormValues,
): SourcingFormValidationError | undefined {
    const parsed = buildSourcingFormSchema(order).safeParse(value)
    if (parsed.success) return undefined
    const fields: Record<string, string> = {}
    for (const issue of parsed.error.issues) {
        const name = zodPathToFieldName(issue.path)
        if (!name || fields[name]) continue
        fields[name] = humanizeIssueMessage(issue)
    }
    return { fields }
}

/**
 * 从 TanStack Form `getAllErrors()` 抽出可展示的去重文案。
 *
 * @param all 表单与字段错误。
 * @returns 按出现顺序去重后的错误文案。
 */
export function collectSourcingErrorMessages(all: {
    form: { errors: readonly unknown[] }
    fields: Record<string, { errors: readonly unknown[] }>
}): string[] {
    const seen = new Set<string>()
    const messages: string[] = []
    const visit = (value: unknown) => {
        for (const text of flattenErrorText(value)) {
            if (seen.has(text)) continue
            seen.add(text)
            messages.push(text)
        }
    }
    for (const error of all.form.errors) visit(error)
    for (const field of Object.values(all.fields)) {
        for (const error of field.errors) visit(error)
    }
    return messages
}

function zodPathToFieldName(path: readonly PropertyKey[]): string {
    let name = ""
    for (const segment of path) {
        const asNumber = typeof segment === "number" ? segment : Number(segment)
        const isIndex =
            typeof segment === "number" ||
            (typeof segment === "string" &&
                segment !== "" &&
                Number.isInteger(asNumber))
        if (isIndex && name) {
            name += `[${asNumber}]`
        } else {
            name += name ? `.${String(segment)}` : String(segment)
        }
    }
    return name
}

function humanizeIssueMessage(issue: {
    path: readonly PropertyKey[]
    message: string
}): string {
    if (issue.message && !issue.message.startsWith("Invalid input")) {
        return issue.message
    }
    const key = String(issue.path.at(-1) ?? "")
    if (key === "basisId") return "请选择履约方案"
    if (key === "quantity") return "请填写本次分配数量"
    if (key === "expectedDeliveryDate") return "请选择预计交付日"
    if (key === "salesOrderId") return "请选择来源销售单"
    return "填写内容不完整，请检查供给方案和分配数量。"
}

function isBusinessDate(value: string): boolean {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return false
    const date = new Date(`${value}T00:00:00Z`)
    return (
        !Number.isNaN(date.valueOf()) &&
        date.toISOString().slice(0, 10) === value
    )
}

function displayErrorText(value: string): string[] {
    const trimmed = value.trim()
    if (!trimmed) return []
    if (trimmed.startsWith("Invalid input")) {
        return ["填写内容不完整，请检查供给方案和分配数量。"]
    }
    return [trimmed]
}

function flattenErrorText(value: unknown): string[] {
    if (value == null) return []
    if (typeof value === "string") {
        return displayErrorText(value)
    }
    if (typeof value !== "object") return []
    if (
        "message" in value &&
        typeof (value as { message: unknown }).message === "string"
    ) {
        return displayErrorText((value as { message: string }).message)
    }
    if (
        "fields" in value &&
        (value as { fields: unknown }).fields &&
        typeof (value as { fields: unknown }).fields === "object"
    ) {
        return Object.values(
            (value as { fields: Record<string, unknown> }).fields,
        ).flatMap(flattenErrorText)
    }
    return []
}
