import { z } from "zod"

import {
    findSourcingOption,
    sourcingQuantityError,
    type SourcingSalesOrder,
} from "@/features/purchase-orders/lib/purchase-order-create-model"

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
            salesOrderId: z.string().trim().min(1, "请选择来源销售单"),
            lines: z.array(
                z.object({
                    salesOrderLineId: z.string().min(1),
                    selected: z.boolean(),
                    quantity: z.string().trim(),
                    supplierId: z.string(),
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
                if (!line.supplierId) {
                    context.addIssue({
                        code: "custom",
                        path: ["lines", index, "supplierId"],
                        message: "请选择供应商",
                    })
                    return
                }
                if (!findSourcingOption(product, line.supplierId)) {
                    context.addIssue({
                        code: "custom",
                        path: ["lines", index, "supplierId"],
                        message: "该供应商当前没有合格供给",
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
                        message: quantityMessage,
                    })
                }
            })
        })
}
