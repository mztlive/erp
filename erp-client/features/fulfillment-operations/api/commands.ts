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
import { stripDeliveryApprovalField } from "@/features/fulfillment-operations/lib/delivery-no-approval"
import { stripElectronicDeliveryApprovalField } from "@/features/fulfillment-operations/lib/electronic-delivery-no-approval"
import { stripPurchaseReceiptApprovalField } from "@/features/fulfillment-operations/lib/purchase-receipt-no-approval"
import { stripServiceFulfillmentApprovalField } from "@/features/fulfillment-operations/lib/service-fulfillment-no-approval"
import type {
    BackendDelivery,
    BackendDeliveryDetail,
    BackendElectronicDelivery,
    BackendPurchaseReceipt,
    BackendPurchaseReceiptDetail,
    BackendServiceFulfillment,
} from "./documents"
import {
    formalFromDelivery,
    formalFromElectronic,
    formalFromReceipt,
    formalFromService,
} from "./outcomes"

/**
 * 保存履约草稿。PurchaseReceipt 为 NO_APPROVAL，入库保存不绑定、不启动审批。
 * Delivery 为 NO_APPROVAL，仓发/直发保存不绑定、不启动审批。
 * ElectronicDelivery 为 NO_APPROVAL，电子交付无草稿保存命令，也不绑定或启动审批。
 * ServiceFulfillment 为 NO_APPROVAL，服务履约无草稿保存命令，也不绑定或启动审批。
 * CustomerAcceptance 为 NO_APPROVAL，本命令不保存客户验收，也不绑定或启动审批。
 *
 * @param input 保存命令。
 * @returns 新的单据版本。
 */
export async function saveFulfillmentOperation(
    input: SaveFulfillmentOperationCommand,
): Promise<{ editVersion: number }> {
    const draft = input.draft
    if (draft.type === "RECEIPT") {
        const updated = stripPurchaseReceiptApprovalField(
            await apiPut<BackendPurchaseReceipt>(
                `/admin/purchase-receipts/${encodeURIComponent(input.operationId)}`,
                {
                    version: input.expectedDocumentVersion,
                    expected_source_version: input.expectedSourceVersion,
                    idempotency_key: input.idempotencyKey,
                    warehouse_id: draft.warehouseId || undefined,
                },
            ),
        )
        return { editVersion: updated.version }
    }
    if (draft.type === "WAREHOUSE_SHIP" || draft.type === "SUPPLIER_DIRECT") {
        const updated = stripDeliveryApprovalField(
            await apiPut<BackendDelivery>(
                `/admin/deliveries/${encodeURIComponent(input.operationId)}`,
                {
                    version: input.expectedDocumentVersion,
                    expected_source_version: input.expectedSourceVersion,
                    idempotency_key: input.idempotencyKey,
                    carrier: draft.carrier || undefined,
                    tracking_no: draft.trackingNo || undefined,
                },
            ),
        )
        return { editVersion: updated.version }
    }
    throw new Error("电子交付与服务履约草稿不支持保存；请直接确认正式单据")
}

/**
 * 确认正式单据。PurchaseReceipt 为 NO_APPROVAL，入库确认直接过账，不提交审批。
 * Delivery 为 NO_APPROVAL，仓发/直发确认直接过账，不提交审批。
 * ElectronicDelivery 为 NO_APPROVAL，电子交付确认直接落账，不提交审批。
 * ServiceFulfillment 为 NO_APPROVAL，服务履约确认直接落账，不提交审批。
 * CustomerAcceptance 为 NO_APPROVAL，确认结果只交接销售验收，不提交审批。
 *
 * @param input 确认命令。
 * @returns 成功/失败/待确认结果；入库、仓发、电子交付与服务履约成功结果不含审批区。
 */
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
                // PurchaseReceipt 为 NO_APPROVAL，确认前丢弃误带的审批绑定。
                receipt = stripPurchaseReceiptApprovalField(detail.receipt)
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
                const updated = stripPurchaseReceiptApprovalField(
                    await apiPut<BackendPurchaseReceipt>(
                        `/admin/purchase-receipts/${encodeURIComponent(receiptId)}`,
                        {
                            version: input.expectedDocumentVersion,
                            expected_source_version:
                                input.expectedSourceVersion,
                            idempotency_key: input.idempotencyKey,
                            warehouse_id: draft.warehouseId,
                        },
                    ),
                )
                commandVersion = updated.version
            }

            const posted = stripPurchaseReceiptApprovalField(
                await apiPost<BackendPurchaseReceipt>(
                    `/admin/purchase-receipts/${encodeURIComponent(receiptId)}/post`,
                    {
                        version: commandVersion,
                        expected_source_version: input.expectedSourceVersion,
                        idempotency_key: input.idempotencyKey,
                    },
                ),
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
                // Delivery 为 NO_APPROVAL，确认前丢弃误带的审批绑定。
                delivery = stripDeliveryApprovalField(detail.delivery)
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

            const updated = stripDeliveryApprovalField(
                await apiPut<BackendDelivery>(
                    `/admin/deliveries/${encodeURIComponent(delivery.id)}`,
                    {
                        version: input.expectedDocumentVersion,
                        expected_source_version: input.expectedSourceVersion,
                        idempotency_key: input.idempotencyKey,
                        carrier: draft.carrier || undefined,
                        tracking_no: draft.trackingNo || undefined,
                    },
                ),
            )
            const posted = stripDeliveryApprovalField(
                await apiPost<BackendDelivery>(
                    `/admin/deliveries/${encodeURIComponent(delivery.id)}/post`,
                    {
                        version: updated.version,
                        expected_source_version: input.expectedSourceVersion,
                        idempotency_key: input.idempotencyKey,
                    },
                ),
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
            // ElectronicDelivery 为 NO_APPROVAL，确认后丢弃误带的审批绑定。
            const confirmed = stripElectronicDeliveryApprovalField(
                await apiPost<BackendElectronicDelivery>(
                    `/admin/electronic-deliveries/${encodeURIComponent(input.operationId)}/confirm`,
                    {
                        version: input.expectedDocumentVersion,
                        expected_source_version: input.expectedSourceVersion,
                        idempotency_key: input.idempotencyKey,
                    },
                ),
            )
            return {
                status: "succeeded",
                outcome: formalFromElectronic(
                    confirmed,
                    draft,
                    input.operationId,
                ),
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
        // ServiceFulfillment 为 NO_APPROVAL，确认后丢弃误带的审批绑定。
        const confirmed = stripServiceFulfillmentApprovalField(
            await apiPost<BackendServiceFulfillment>(
                `/admin/service-fulfillments/${encodeURIComponent(input.operationId)}/confirm`,
                {
                    version: input.expectedDocumentVersion,
                    expected_source_version: input.expectedSourceVersion,
                    idempotency_key: input.idempotencyKey,
                },
            ),
        )
        return {
            status: "succeeded",
            outcome: formalFromService(confirmed, draft, input.operationId),
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

/**
 * 复核暂未确认的处理结果。PurchaseReceipt 为 NO_APPROVAL，已入库事实不含审批绑定。
 * Delivery 为 NO_APPROVAL，已发货事实不含审批绑定。
 * ElectronicDelivery 为 NO_APPROVAL，已确认电子交付不含审批绑定。
 * ServiceFulfillment 为 NO_APPROVAL，已确认服务履约不含审批绑定。
 * CustomerAcceptance 为 NO_APPROVAL，复核结果交接客户验收时不含审批绑定。
 *
 * @param input 复核命令。
 * @returns 已过账则返回正式结果，否则保持未知。
 */
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
                const receipt = stripPurchaseReceiptApprovalField(d.receipt)
                if (receipt.status === "POSTED") {
                    return {
                        status: "succeeded",
                        outcome: {
                            kind: "POSTED",
                            operationId: input.operationId,
                            factType: "PURCHASE_RECEIPT",
                            factId: receipt.id,
                            factNo: receipt.receipt_no,
                            formalStatus: "POSTED",
                            occurredAt:
                                secsToIso(receipt.posted_at) || nowIso(),
                            operationType: "RECEIPT",
                            inventoryDelta: [],
                            reservationDelta: [],
                            remainingByLine: [],
                            acceptanceRequired: false,
                            acceptanceNextStep: "",
                            inventoryImpactSummary: "",
                            reference: receipt.receipt_no,
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
                const delivery = stripDeliveryApprovalField(d.delivery)
                if (delivery.status === "SHIPPED" || delivery.status === "SIGNED") {
                    return {
                        status: "succeeded",
                        outcome: {
                            kind: "POSTED",
                            operationId: input.operationId,
                            factType: "DELIVERY",
                            factId: delivery.id,
                            factNo: delivery.delivery_no,
                            formalStatus: delivery.status,
                            occurredAt:
                                secsToIso(delivery.shipped_at) || nowIso(),
                            operationType:
                                delivery.delivery_type === "SUPPLIER_DIRECT"
                                    ? "SUPPLIER_DIRECT"
                                    : "WAREHOUSE_SHIP",
                            inventoryDelta: [],
                            reservationDelta: [],
                            remainingByLine: [],
                            acceptanceRequired: true,
                            acceptanceNextStep: "",
                            inventoryImpactSummary: "",
                            reference: delivery.delivery_no,
                            salesOrderId: delivery.sales_order_id,
                            salesOrderNo: delivery.sales_order_id,
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
