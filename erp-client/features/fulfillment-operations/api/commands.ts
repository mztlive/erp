/**
 * W09 履约单据处理 · 单据命令：保存草稿、确认正式单据、复核「暂无法确认」。
 * 直接对接到各领域强类型命令；不得用客户端责任状态补足缺失的任务合同。
 */

import { apiGet, apiPost, apiPut } from "@/lib/api"
import type {
    FormalActionResponse,
    PostFulfillmentOperationCommand,
    ResolveFulfillmentOperationCommand,
    SaveFulfillmentOperationCommand,
} from "@/features/fulfillment-operations/types"
import {
    isApiError,
    nowIso,
    secsToIso,
} from "@/features/fulfillment-operations/lib/projection"
import type {
    BackendDelivery,
    BackendDeliveryDetail,
    BackendElectronicDelivery,
    BackendPurchaseReceipt,
    BackendPurchaseReceiptDetail,
    BackendServiceFulfillment,
} from "./documents"
import { formalFromDelivery, formalFromReceipt } from "./outcomes"

export async function saveFulfillmentOperation(
    input: SaveFulfillmentOperationCommand,
): Promise<{ editVersion: number }> {
    const draft = input.draft
    if (draft.type === "RECEIPT") {
        const updated = await apiPut<BackendPurchaseReceipt>(
            `/admin/purchase-receipts/${encodeURIComponent(input.operationId)}`,
            {
                version: input.expectedDocumentVersion,
                expected_source_version: input.expectedSourceVersion,
                idempotency_key: input.idempotencyKey,
                warehouse_id: draft.warehouseId || undefined,
            },
        )
        return { editVersion: updated.version }
    }
    if (draft.type === "WAREHOUSE_SHIP" || draft.type === "SUPPLIER_DIRECT") {
        const updated = await apiPut<BackendDelivery>(
            `/admin/deliveries/${encodeURIComponent(input.operationId)}`,
            {
                version: input.expectedDocumentVersion,
                expected_source_version: input.expectedSourceVersion,
                idempotency_key: input.idempotencyKey,
                carrier: draft.carrier || undefined,
                tracking_no: draft.trackingNo || undefined,
            },
        )
        return { editVersion: updated.version }
    }
    throw new Error("电子交付与服务履约草稿不支持保存；请直接确认正式单据")
}

export async function postFulfillmentOperation(
    input: PostFulfillmentOperationCommand,
): Promise<FormalActionResponse> {
    const draft = input.draft

    try {
        if (draft.type === "RECEIPT") {
            // Prefer post existing draft document; if not found, create then post
            const receiptId = input.operationId
            let receipt: BackendPurchaseReceipt | null = null
            try {
                const detail = await apiGet<BackendPurchaseReceiptDetail>(
                    `/admin/purchase-receipts/${encodeURIComponent(input.operationId)}`,
                )
                receipt = detail.receipt
            } catch (error) {
                if (!(isApiError(error) && error.status === 404)) throw error
            }

            if (!receipt) {
                if (!draft.lines.length) {
                    return {
                        status: "failed",
                        code: "VALIDATION_BLOCKED",
                        message: "入库明细不能为空",
                    }
                }
                // Need purchase_order_id — not always on draft; require from source via prior queue
                return {
                    status: "failed",
                    code: "BACKEND_GAP",
                    message:
                        "未找到入库草稿。请从采购上下文创建入库单后再确认（队列投影与创建链路待后端补齐）。",
                }
            }

            let commandVersion = input.expectedDocumentVersion
            if (
                receipt.warehouse_id !== draft.warehouseId &&
                draft.warehouseId
            ) {
                const updated = await apiPut<BackendPurchaseReceipt>(
                    `/admin/purchase-receipts/${encodeURIComponent(receiptId)}`,
                    {
                        version: input.expectedDocumentVersion,
                        expected_source_version: input.expectedSourceVersion,
                        idempotency_key: input.idempotencyKey,
                        warehouse_id: draft.warehouseId,
                    },
                )
                commandVersion = updated.version
            }

            const posted = await apiPost<BackendPurchaseReceipt>(
                `/admin/purchase-receipts/${encodeURIComponent(receiptId)}/post`,
                {
                    version: commandVersion,
                    expected_source_version: input.expectedSourceVersion,
                    idempotency_key: input.idempotencyKey,
                },
            )
            return {
                status: "succeeded",
                outcome: formalFromReceipt(posted, draft, input.operationId),
            }
        }

        if (
            draft.type === "WAREHOUSE_SHIP" ||
            draft.type === "SUPPLIER_DIRECT"
        ) {
            let delivery: BackendDelivery | null = null
            try {
                const detail = await apiGet<BackendDeliveryDetail>(
                    `/admin/deliveries/${encodeURIComponent(input.operationId)}`,
                )
                delivery = detail.delivery
            } catch (error) {
                if (!(isApiError(error) && error.status === 404)) throw error
            }

            if (!delivery) {
                return {
                    status: "failed",
                    code: "DOCUMENT_NOT_FOUND",
                    message: "发货草稿已不存在，请刷新后重新选择单据",
                }
            }

            const updated = await apiPut<BackendDelivery>(
                `/admin/deliveries/${encodeURIComponent(delivery.id)}`,
                {
                    version: input.expectedDocumentVersion,
                    expected_source_version: input.expectedSourceVersion,
                    idempotency_key: input.idempotencyKey,
                    carrier: draft.carrier || undefined,
                    tracking_no: draft.trackingNo || undefined,
                },
            )
            const posted = await apiPost<BackendDelivery>(
                `/admin/deliveries/${encodeURIComponent(delivery.id)}/post`,
                {
                    version: updated.version,
                    expected_source_version: input.expectedSourceVersion,
                    idempotency_key: input.idempotencyKey,
                },
            )
            return {
                status: "succeeded",
                outcome: formalFromDelivery(posted, draft, input.operationId),
            }
        }

        if (draft.type === "ELECTRONIC") {
            const line = draft.lines[0]
            if (!line) {
                return {
                    status: "failed",
                    code: "VALIDATION_BLOCKED",
                    message: "交付明细不能为空",
                }
            }
            const confirmed = await apiPost<BackendElectronicDelivery>(
                `/admin/electronic-deliveries/${encodeURIComponent(input.operationId)}/confirm`,
                {
                    version: input.expectedDocumentVersion,
                    expected_source_version: input.expectedSourceVersion,
                    idempotency_key: input.idempotencyKey,
                },
            )
            return {
                status: "succeeded",
                outcome: {
                    kind: "POSTED",
                    operationId: input.operationId,
                    factType: "ELECTRONIC_DELIVERY",
                    factId: confirmed.id,
                    factNo: confirmed.fulfillment_no,
                    formalStatus:
                        confirmed.result === "FAILED" ? "FAILED" : "CONFIRMED",
                    occurredAt: secsToIso(confirmed.occurred_at) || nowIso(),
                    operationType: "ELECTRONIC",
                    inventoryDelta: [],
                    reservationDelta: [],
                    remainingByLine: [],
                    acceptanceRequired: confirmed.result !== "FAILED",
                    acceptanceNextStep:
                        "电子交付已确认，不影响自有库存。请销售在客户验收登记。",
                    inventoryImpactSummary: "不影响自有库存。",
                    reference: confirmed.fulfillment_no,
                    salesOrderId: "",
                    salesOrderNo: "",
                },
            }
        }

        // SERVICE
        const line = draft.lines[0]
        if (!line) {
            return {
                status: "failed",
                code: "VALIDATION_BLOCKED",
                message: "服务明细不能为空",
            }
        }
        const confirmed = await apiPost<BackendServiceFulfillment>(
            `/admin/service-fulfillments/${encodeURIComponent(input.operationId)}/confirm`,
            {
                version: input.expectedDocumentVersion,
                expected_source_version: input.expectedSourceVersion,
                idempotency_key: input.idempotencyKey,
            },
        )
        return {
            status: "succeeded",
            outcome: {
                kind: "POSTED",
                operationId: input.operationId,
                factType: "SERVICE_FULFILLMENT",
                factId: confirmed.id,
                factNo: confirmed.fulfillment_no,
                formalStatus:
                    confirmed.result === "FAILED" ? "FAILED" : "CONFIRMED",
                occurredAt: secsToIso(confirmed.occurred_at) || nowIso(),
                operationType: "SERVICE",
                inventoryDelta: [],
                reservationDelta: [],
                remainingByLine: [],
                acceptanceRequired: confirmed.result !== "FAILED",
                acceptanceNextStep: "服务履约已确认。请销售在客户验收登记。",
                inventoryImpactSummary: "不影响自有库存。",
                reference: confirmed.fulfillment_no,
                salesOrderId: "",
                salesOrderNo: "",
            },
        }
    } catch (error) {
        if (isApiError(error)) {
            if (
                error.status === 500 &&
                typeof error.message === "string" &&
                error.message.includes("暂无法确认")
            ) {
                return {
                    status: "unknown",
                    message: error.message,
                    idempotencyKey: input.idempotencyKey,
                }
            }
            if (error.status === 409) {
                return {
                    status: "failed",
                    code: "SUBJECT_VERSION_MISMATCH",
                    message: "数据已变更，请刷新后重试",
                }
            }
            return {
                status: "failed",
                code: String(error.status ?? "ERROR"),
                message: error.message,
            }
        }
        throw error
    }
}

export async function resolveUnknownFulfillmentResult(
    input: ResolveFulfillmentOperationCommand,
): Promise<FormalActionResponse> {
    // Probe document status for posted outcomes
    const probes: Array<() => Promise<FormalActionResponse | null>> = [
        async () => {
            try {
                const d = await apiGet<BackendPurchaseReceiptDetail>(
                    `/admin/purchase-receipts/${encodeURIComponent(input.operationId)}`,
                )
                if (d.receipt.status === "POSTED") {
                    return {
                        status: "succeeded",
                        outcome: {
                            kind: "POSTED",
                            operationId: input.operationId,
                            factType: "PURCHASE_RECEIPT",
                            factId: d.receipt.id,
                            factNo: d.receipt.receipt_no,
                            formalStatus: "POSTED",
                            occurredAt:
                                secsToIso(d.receipt.posted_at) || nowIso(),
                            operationType: "RECEIPT",
                            inventoryDelta: [],
                            reservationDelta: [],
                            remainingByLine: [],
                            acceptanceRequired: false,
                            acceptanceNextStep: "",
                            inventoryImpactSummary: "",
                            reference: d.receipt.receipt_no,
                            salesOrderId: "",
                            salesOrderNo: "",
                        },
                    }
                }
            } catch {
                /* continue */
            }
            return null
        },
        async () => {
            try {
                const d = await apiGet<BackendDeliveryDetail>(
                    `/admin/deliveries/${encodeURIComponent(input.operationId)}`,
                )
                if (
                    d.delivery.status === "SHIPPED" ||
                    d.delivery.status === "SIGNED"
                ) {
                    return {
                        status: "succeeded",
                        outcome: {
                            kind: "POSTED",
                            operationId: input.operationId,
                            factType: "DELIVERY",
                            factId: d.delivery.id,
                            factNo: d.delivery.delivery_no,
                            formalStatus: d.delivery.status,
                            occurredAt:
                                secsToIso(d.delivery.shipped_at) || nowIso(),
                            operationType:
                                d.delivery.delivery_type === "SUPPLIER_DIRECT"
                                    ? "SUPPLIER_DIRECT"
                                    : "WAREHOUSE_SHIP",
                            inventoryDelta: [],
                            reservationDelta: [],
                            remainingByLine: [],
                            acceptanceRequired: true,
                            acceptanceNextStep: "",
                            inventoryImpactSummary: "",
                            reference: d.delivery.delivery_no,
                            salesOrderId: d.delivery.sales_order_id,
                            salesOrderNo: d.delivery.sales_order_id,
                        },
                    }
                }
            } catch {
                /* continue */
            }
            return null
        },
    ]

    for (const probe of probes) {
        const hit = await probe()
        if (hit) return hit
    }

    return {
        status: "failed",
        code: "NO_PENDING",
        message: "未找到该单据对应的处理中请求",
    }
}
