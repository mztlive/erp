import { z } from "zod"

import {
    findSourcingOption,
    sourcingQuantityError,
    type SourcingLineInput,
    type SourcingSalesOrder,
} from "@/features/purchase-orders/lib/purchase-order-create-model"

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
    const maximumByLineId = new Map(
        order?.lines.flatMap((line) =>
            line.options.map((option) => [
                `${line.salesOrderLineId}:${option.supplierId}`,
                option.maxCreateQuantity,
            ]),
        ) ?? [],
    )
    return z
        .object({
            salesOrderId: textField.pipe(z.string().min(1, "请选择来源销售单")),
            lines: z.array(
                z.object({
                    salesOrderLineId: textField,
                    selected: flagField,
                    quantity: textField,
                    supplierId: textField,
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
                    message: "请至少选择一条本次采购明细",
                })
                return
            }
            selected.forEach((line) => {
                const index = value.lines.findIndex(
                    (candidate) =>
                        candidate.salesOrderLineId === line.salesOrderLineId,
                )
                const product = order.lines.find(
                    (candidate) =>
                        candidate.salesOrderLineId === line.salesOrderLineId,
                )
                const itemLabel = product?.itemName
                if (!line.supplierId) {
                    context.addIssue({
                        code: "custom",
                        path: ["lines", index, "supplierId"],
                        message: itemLabel
                            ? `${itemLabel}：请选择供应商`
                            : "请选择供应商",
                    })
                    return
                }
                if (!findSourcingOption(product, line.supplierId)) {
                    context.addIssue({
                        code: "custom",
                        path: ["lines", index, "supplierId"],
                        message: itemLabel
                            ? `${itemLabel}：该供应商当前没有合格供给`
                            : "该供应商当前没有合格供给",
                    })
                    return
                }
                const maximum =
                    maximumByLineId.get(
                        `${line.salesOrderLineId}:${line.supplierId}`,
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
            })
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
    if (key === "supplierId") return "请选择供应商"
    if (key === "quantity") return "请填写本次采购数量"
    if (key === "salesOrderId") return "请选择来源销售单"
    return "填写内容不完整，请检查供应商和采购数量。"
}

function displayErrorText(value: string): string[] {
    const trimmed = value.trim()
    if (!trimmed) return []
    if (trimmed.startsWith("Invalid input")) {
        return ["填写内容不完整，请检查供应商和采购数量。"]
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
