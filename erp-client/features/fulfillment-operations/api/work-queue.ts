/** W09 服务端履约责任队列 Wire 合同、运行时解码与页面领域映射。 */

import { z } from "zod"

import type { FulfillmentOperation } from "@/features/fulfillment-operations/types"
import {
    baseOperation,
    emptySourceLine,
    nowIso,
    secsToIso,
} from "@/features/fulfillment-operations/lib/projection"

const operationTypeSchema = z.enum([
    "RECEIPT",
    "WAREHOUSE_SHIP",
    "SUPPLIER_DIRECT",
    "ELECTRONIC",
    "SERVICE",
])

const queueItemSchema = z
    .object({
        work_item_id: z.string().min(1),
        task_version: z.string().min(1),
        source_version: z.string().min(1),
        owner_role: z.string().min(1),
        owner_organization_id: z.string().min(1),
        priority: z.enum(["urgent", "high", "normal", "low"]),
        reason_code: z.string(),
        impact_summary: z.string(),
        operation_id: z.string().min(1),
        operation_type: operationTypeSchema,
        business_object_type: z.string().min(1),
        summary: z.string(),
        edit_version: z.number().int().positive(),
        due_at: z.number().int(),
        sales_order_id: z.string().optional(),
        sales_order_no: z.string().optional(),
        purchase_order_id: z.string().optional(),
        purchase_order_no: z.string().optional(),
        warehouse_id: z.string().optional(),
        warehouse_label: z.string().optional(),
        sales_order_line_id: z.string().optional(),
        purchase_line_sales_allocation_id: z.string().optional(),
        quantity: z.string().optional(),
        result: z.string().optional(),
        carrier: z.string().optional(),
        tracking_no: z.string().optional(),
        gate_state: z.enum(["SATISFIED", "BLOCKED", "NOT_APPLICABLE"]),
    })
    .strict()

const fulfillmentQueuePageSchema = z
    .object({
        items: z.array(queueItemSchema),
        total: z.number().int().nonnegative(),
        page: z.number().int().positive(),
        page_size: z.number().int().positive().max(100),
        queue_context_id: z.string().min(1),
        visible_types: z.array(operationTypeSchema),
        metrics: z.array(
            z
                .object({
                    operation_type: operationTypeSchema,
                    count: z.number().int().nonnegative(),
                })
                .strict(),
        ),
        warehouse_options: z.array(
            z
                .object({
                    id: z.string().min(1),
                    label: z.string().min(1),
                })
                .strict(),
        ),
        as_of: z.number().int(),
    })
    .strict()

export type FulfillmentQueueWirePage = z.infer<
    typeof fulfillmentQueuePageSchema
>
type FulfillmentQueueWireItem = z.infer<typeof queueItemSchema>

/** 关键财务/履约作业面不得用泛型断言掩盖 Wire 字段漂移。 */
export function decodeFulfillmentQueuePage(
    input: unknown,
): FulfillmentQueueWirePage {
    const result = fulfillmentQueuePageSchema.safeParse(input)
    if (result.success) return result.data
    throw new Error(`履约队列响应契约不匹配：${z.prettifyError(result.error)}`)
}

const PRIORITY_VALUE = {
    urgent: 4,
    high: 3,
    normal: 2,
    low: 1,
} as const

const OWNER_ROLE_LABEL: Record<string, string> = {
    warehouse_inbound_handler: "仓库入库经办",
    warehouse_outbound_handler: "仓库发货经办",
    purchase_order_owner: "采购单责任人",
}

function sourceLine(row: FulfillmentQueueWireItem) {
    if (
        !row.sales_order_line_id ||
        !row.purchase_line_sales_allocation_id ||
        !row.quantity
    ) {
        return []
    }
    return [
        emptySourceLine({
            lineId: row.sales_order_line_id,
            salesOrderLineId: row.sales_order_line_id,
            purchaseLineSalesAllocationId:
                row.purchase_line_sales_allocation_id,
            orderedQuantity: row.quantity,
            remainingQuantity: row.quantity,
        }),
    ]
}

function electronicResult(
    value: string | undefined,
): "SUCCESS" | "PARTIAL" | "FAILED" {
    if (value === "PARTIAL" || value === "PARTIAL_SUCCESS") return "PARTIAL"
    if (value === "FAILED" || value === "FAILURE") return "FAILED"
    return "SUCCESS"
}

/** 将服务端责任行映射为现有 W09 页面领域模型。 */
export function fulfillmentQueueItemToOperation(
    row: FulfillmentQueueWireItem,
): FulfillmentOperation {
    const dueAt = secsToIso(row.due_at) || nowIso()
    const source = {
        purchaseOrderId: row.purchase_order_id,
        purchaseNo: row.purchase_order_no,
        salesOrderId: row.sales_order_id ?? "",
        salesOrderNo: row.sales_order_no ?? "",
        salesRevisionId: "",
        customerLabel: "",
        warehouseId: row.warehouse_id,
        warehouseLabel: row.warehouse_label,
    }
    const common = {
        operationId: row.operation_id,
        operationType: row.operation_type,
        priority: PRIORITY_VALUE[row.priority],
        dueAt,
        sourceVersion: row.source_version,
        editVersion: row.edit_version,
        responsibleLabel: OWNER_ROLE_LABEL[row.owner_role] ?? row.owner_role,
        summary: row.summary,
        impact: row.impact_summary,
        source,
        gate: {
            state: row.gate_state,
            message:
                row.gate_state === "SATISFIED"
                    ? "作业先决条件已满足"
                    : row.gate_state === "BLOCKED"
                      ? "作业先决条件尚未满足"
                      : "",
        },
    } as const

    switch (row.operation_type) {
        case "RECEIPT":
            return baseOperation({
                ...common,
                operationType: "RECEIPT",
                draft: {
                    type: "RECEIPT",
                    warehouseId: row.warehouse_id ?? "",
                    warehouseLabel: row.warehouse_label ?? "",
                    occurredAt: dueAt.slice(0, 16),
                    lines: [],
                },
            })
        case "WAREHOUSE_SHIP":
            return baseOperation({
                ...common,
                operationType: "WAREHOUSE_SHIP",
                draft: {
                    type: "WAREHOUSE_SHIP",
                    warehouseId: row.warehouse_id ?? "",
                    warehouseLabel: row.warehouse_label ?? "",
                    carrier: row.carrier ?? "",
                    trackingNo: row.tracking_no ?? "",
                    shippedAt: dueAt.slice(0, 16),
                    lines: [],
                },
            })
        case "SUPPLIER_DIRECT":
            return baseOperation({
                ...common,
                operationType: "SUPPLIER_DIRECT",
                draft: {
                    type: "SUPPLIER_DIRECT",
                    carrier: row.carrier ?? "",
                    trackingNo: row.tracking_no ?? "",
                    shippedAt: dueAt.slice(0, 16),
                    lines: [],
                },
            })
        case "ELECTRONIC":
            return baseOperation({
                ...common,
                operationType: "ELECTRONIC",
                lines: sourceLine(row),
                draft: {
                    type: "ELECTRONIC",
                    occurredAt: dueAt.slice(0, 16),
                    recipientMasked: "",
                    result: electronicResult(row.result),
                    lines:
                        row.sales_order_line_id &&
                        row.purchase_line_sales_allocation_id &&
                        row.quantity
                            ? [
                                  {
                                      salesOrderLineId: row.sales_order_line_id,
                                      purchaseLineSalesAllocationId:
                                          row.purchase_line_sales_allocation_id,
                                      quantity: row.quantity,
                                  },
                              ]
                            : [],
                },
            })
        case "SERVICE":
            return baseOperation({
                ...common,
                operationType: "SERVICE",
                lines: sourceLine(row),
                draft: {
                    type: "SERVICE",
                    startedAt: "",
                    endedAt: "",
                    serviceLocation: "",
                    result: "",
                    completionNote: "",
                    evidenceAttachmentId: "",
                    lines:
                        row.sales_order_line_id &&
                        row.purchase_line_sales_allocation_id &&
                        row.quantity
                            ? [
                                  {
                                      salesOrderLineId: row.sales_order_line_id,
                                      purchaseLineSalesAllocationId:
                                          row.purchase_line_sales_allocation_id,
                                      quantity: row.quantity,
                                  },
                              ]
                            : [],
                },
            })
    }
}
