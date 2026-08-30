import type {
    FulfillmentFormalOutcome,
    FulfillmentOperation,
    FulfillmentQueueView,
} from "@/features/fulfillment-operations/types"

/** 可配置的履约单据夹具；默认是可直接执行的入库单据。 */
export function makeOperation(
    overrides: {
        operationId?: string
        operationType?: FulfillmentOperation["operationType"]
        source?: Partial<FulfillmentOperation["source"]>
        gate?: Partial<FulfillmentOperation["gate"]>
        actionBlockers?: FulfillmentOperation["actionBlockers"]
        dueLabel?: string
        overdue?: boolean
    } = {},
): FulfillmentOperation {
    return {
        operationId: overrides.operationId ?? "op_1",
        operationType: overrides.operationType ?? "RECEIPT",
        priority: 1,
        dueAt: "2026-08-14T10:00:00.000Z",
        dueLabel: overrides.dueLabel ?? "今天 10:00",
        overdue: overrides.overdue ?? false,
        statusLabel: "待确认",
        statusTone: "warning",
        responsibleLabel: "仓储 · 周航",
        sourceVersion: "sv_1",
        editVersion: 3,
        source: {
            salesOrderId: "so_1",
            salesOrderNo: "SO-2026-001",
            salesRevisionId: "sr_1",
            purchaseOrderId: "po_1",
            purchaseNo: "PO-2026-001",
            customerLabel: "演示客户",
            supplierLabel: "演示供应商",
            warehouseId: "wh_1",
            warehouseLabel: "中心仓",
            ...overrides.source,
        },
        gate: {
            state: "SATISFIED",
            message: "货款已到，可以收货",
            effectivePaidAmount: "1000",
            requiredAmount: "1000",
            ...overrides.gate,
        },
        lines: [
            {
                lineId: "line_1",
                salesOrderLineId: "sol_1",
                purchaseRevisionLineId: "prl_1",
                itemName: "演示商品",
                skuCode: "SKU-1",
                unitCode: "件",
                orderedQuantity: "10",
                remainingQuantity: "10",
            },
        ],
        draft: {
            type: "RECEIPT",
            warehouseId: "wh_1",
            warehouseLabel: "中心仓",
            occurredAt: "2026-08-14T09:00:00.000Z",
            lines: [
                {
                    purchaseRevisionLineId: "prl_1",
                    receivedQuantity: "10",
                    qualifiedQuantity: "10",
                    rejectedQuantity: "0",
                    qualityResult: "PASS",
                },
            ],
        },
        summary: "待入库 10 件",
        impact: "入库后为销售单留货",
        actionBlockers: overrides.actionBlockers ?? [],
    }
}

export function makeQueueView(
    operations: FulfillmentOperation[],
    overrides: {
        currentOperationId?: string
        canExecute?: boolean
        visibleTypes?: FulfillmentQueueView["context"]["visibleTypes"]
        roleLabel?: string
        total?: number
        emptyReason?: FulfillmentQueueView["emptyReason"]
    } = {},
): FulfillmentQueueView {
    return {
        context: {
            position: 1,
            total: overrides.total ?? operations.length,
            page: 1,
            pageSize: 20,
            totalPages: 1,
            currentOperationId: overrides.currentOperationId,
            filterSummary: "入库 · 全部",
            warehouseOptions: [{ value: "wh_1", label: "中心仓" }],
            visibleTypes: overrides.visibleTypes ?? (["RECEIPT"] as const),
            roleLabel: overrides.roleLabel ?? "仓储经办",
            viewerLabel: "周航",
            canExecute: overrides.canExecute ?? true,
            snapshotUpdatedAt: "2026-08-14T10:00:00.000Z",
        },
        metrics: [],
        operations,
        current:
            operations.find(
                (op) => op.operationId === overrides.currentOperationId,
            ) ?? operations[0],
        emptyReason: overrides.emptyReason,
        preferences: { autoNextDefault: true },
    }
}

export function makePostedOutcome(
    overrides: Partial<FulfillmentFormalOutcome> = {},
): FulfillmentFormalOutcome {
    return {
        kind: "POSTED",
        operationId: "op_1",
        factType: "PURCHASE_RECEIPT",
        factId: "fact_1",
        factNo: "RK-2026-001",
        formalStatus: "POSTED",
        occurredAt: "2026-08-14T10:00:00.000Z",
        operationType: "RECEIPT",
        inventoryDelta: [],
        reservationDelta: [],
        remainingByLine: [],
        acceptanceRequired: false,
        acceptanceNextStep: "由销售登记客户验收",
        inventoryImpactSummary: "中心仓 +10",
        reference: "RK-2026-001",
        salesOrderId: "so_1",
        salesOrderNo: "SO-2026-001",
        ...overrides,
    }
}
