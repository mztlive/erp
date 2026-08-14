import type { UseMutationResult } from "@tanstack/react-query"
import { vi } from "vitest"

import type { SupplierOrderDetailView } from "@/features/supplier-orders/types"

/** 中心页 hooks 测试共用的数据与 mutation 构造器。 */

export const makeWorkItem = (
    overrides: Partial<NonNullable<SupplierOrderDetailView["workItem"]>> = {},
): NonNullable<SupplierOrderDetailView["workItem"]> => ({
    workItemId: "wi1",
    taskVersion: "3",
    workItemType: "INTEGRATION_RESULT_UNKNOWN",
    businessObjectType: "SUPPLIER_FULFILLMENT_ORDER",
    businessObjectId: "o1",
    subjectVersion: "v2",
    assignmentMode: "DIRECT",
    processingState: "READY",
    ownerUser: { id: "u1", displayName: "张三" },
    allowedTaskActions: ["RELEASE_TO_TEAM"],
    actionBlockers: [],
    workItemStatus: "OPEN",
    ...overrides,
})

export const makeDetail = (
    overrides: Partial<SupplierOrderDetailView> = {},
): SupplierOrderDetailView => ({
    order: {
        id: "o1",
        orderNo: "SO-1",
        mallOrderId: "m1",
        mallOrderNo: "MO-1",
        paidAt: "2026-08-01T00:00:00.000Z",
        paymentFactKey: "",
        fulfillmentChain: "ERP_AUTOMATED",
        supplierId: "s1",
        supplierName: "供应商甲",
        connectionCode: "C1",
        connectionEnvironment: "production",
        supplyVersion: "SV-12",
        publicationVersion: "PV-3",
        externalOrderNo: "EXT-1",
        fulfillmentStatus: "RESULT_UNKNOWN",
        fulfillmentLabel: "结果未知",
        fulfillmentTone: "warning",
        cancelStatus: "NONE",
        cancelLabel: "未发起",
        cancelTone: "neutral",
        refundStatus: "NONE",
        refundLabel: "未发起",
        refundTone: "neutral",
        lockVersion: 7,
        paymentOccurredNotice: "商城支付已发生",
    },
    items: [
        {
            itemId: "i1",
            mallLineId: "l1",
            productName: "商品",
            skuCode: "SKU-1",
            quantity: "2",
            unit: "件",
            supplierProductId: "SP-1",
            supplierProductName: "供应商商品",
            publicationVersion: "PV-1",
            supplyVersion: "SV-1",
            unitCostGross: "10.5",
            unitCostNet: null,
            inputTaxRate: null,
            snapshotImmutable: true,
        },
    ],
    logistics: {},
    statusHistory: [],
    afterSales: [],
    costs: {
        cumulativeCostGross: "21.00",
        cumulativeCostNet: null,
        costSource: "下单成本快照",
        costVariance: null,
    },
    actions: [],
    address: {
        masked: "•••",
        phoneMasked: "•••",
        recipientMasked: "•••",
        canReveal: true,
    },
    workItem: makeWorkItem(),
    placeActionId: "act1",
    allowedActions: ["OPEN_CENTER", "NOTE", "QUERY_RESULT", "REPLAY"],
    actionBlockers: [],
    freshness: { updatedAt: "2026-08-01T00:00:00.000Z", state: "fresh" },
    ...overrides,
})

export const makeEvidence = (
    overrides: Partial<
        NonNullable<SupplierOrderDetailView["lastInvestigation"]>
    > = {},
): NonNullable<SupplierOrderDetailView["lastInvestigation"]> => ({
    evidenceId: "ev1",
    targetSupplierActionId: "act1",
    outcome: "VERIFIED_TERMINAL",
    outcomeLabel: "处理结果已核实",
    recordedAt: "2026-08-01T00:00:00.000Z",
    canSafeRetry: false,
    summary: "已核实",
    verifiedSupplierActionResultId: "act1",
    verifiedResolution: "ORDER_COMPLETED",
    ...overrides,
})

export function makeMutation<TData, TVariables>() {
    const mutateAsync = vi.fn<(variables: TVariables) => Promise<TData>>()
    return {
        mutateAsync,
    } as unknown as UseMutationResult<TData, Error, TVariables> & {
        mutateAsync: typeof mutateAsync
    }
}
