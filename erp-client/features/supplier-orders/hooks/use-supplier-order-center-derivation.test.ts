import { renderHook } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import {
    deriveSupplierOrderTotals,
    responsibilityOf,
    useSupplierOrderCenterDerivation,
} from "./use-supplier-order-center-derivation"
import type { SupplierOrderDetailView } from "@/features/supplier-orders/types"

const makeWorkItem = (
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

const makeDetail = (
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

const makeEvidence = (
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

describe("responsibilityOf", () => {
    it("maps missing work items to blocked", () => {
        expect(responsibilityOf(undefined, "u1")).toBe("blocked")
    })

    it("maps completed and closed work items", () => {
        expect(
            responsibilityOf(makeWorkItem({ workItemStatus: "COMPLETED" })),
        ).toBe("completed")
        expect(
            responsibilityOf(makeWorkItem({ workItemStatus: "CLOSED" })),
        ).toBe("closed")
    })

    it("maps approval-blocked work items to blocked", () => {
        expect(
            responsibilityOf(
                makeWorkItem({ processingState: "APPROVAL_BLOCKED" }),
            ),
        ).toBe("blocked")
    })

    it("maps ownerless pool work items to pool_available", () => {
        expect(
            responsibilityOf(
                makeWorkItem({ assignmentMode: "POOL", ownerUser: undefined }),
            ),
        ).toBe("pool_available")
    })

    it("distinguishes assigned_to_me from assigned_to_other", () => {
        expect(responsibilityOf(makeWorkItem(), "u1")).toBe("assigned_to_me")
        expect(responsibilityOf(makeWorkItem(), "u2")).toBe("assigned_to_other")
    })
})

describe("deriveSupplierOrderTotals", () => {
    it("sums quantities and costs across items", () => {
        const items = makeDetail().items
        expect(deriveSupplierOrderTotals(items)).toEqual({
            totalQuantity: 2,
            totalCostGross: "21.00",
        })
    })

    it("returns a null cost total when every unit cost is missing", () => {
        const items = makeDetail().items.map((item) => ({
            ...item,
            unitCostGross: null,
        }))
        expect(deriveSupplierOrderTotals(items)).toEqual({
            totalQuantity: 2,
            totalCostGross: null,
        })
    })

    it("treats missing costs as zero when at least one item has a cost", () => {
        const items = makeDetail().items.concat({
            ...makeDetail().items[0],
            itemId: "i2",
            quantity: "1",
            unitCostGross: null,
        })
        expect(deriveSupplierOrderTotals(items)).toEqual({
            totalQuantity: 3,
            totalCostGross: "21.00",
        })
    })
})

describe("useSupplierOrderCenterDerivation", () => {
    it("derives action switches and the result-unknown flag", () => {
        const { result } = renderHook(() =>
            useSupplierOrderCenterDerivation({
                detail: makeDetail(),
                currentUserId: "u1",
            }),
        )
        expect(result.current.responsibilityStatus).toBe("assigned_to_me")
        expect(result.current.canQuery).toBe(true)
        expect(result.current.canReplay).toBe(true)
        expect(result.current.canReveal).toBe(false)
        expect(result.current.isResultUnknown).toBe(true)
        expect(result.current.noQueryCapability).toBe(false)
        expect(result.current.totalQuantity).toBe(2)
        expect(result.current.totalCostGross).toBe("21.00")
    })

    it("flags the no-query-capability blocker", () => {
        const { result } = renderHook(() =>
            useSupplierOrderCenterDerivation({
                detail: makeDetail({
                    actionBlockers: [
                        {
                            action: "QUERY_RESULT",
                            code: "NO_QUERY_CAPABILITY",
                            message: "该供应商无查询能力",
                        },
                    ],
                }),
            }),
        )
        expect(result.current.noQueryCapability).toBe(true)
    })

    it("allows completing a task only with verified terminal evidence", () => {
        const detail = makeDetail({
            allowedActions: [
                "OPEN_CENTER",
                "NOTE",
                "CONFIRM_VERIFIED_TERMINAL_RESULT",
            ],
            lastInvestigation: makeEvidence(),
        })
        const { result } = renderHook(() =>
            useSupplierOrderCenterDerivation({
                detail,
                currentUserId: "u1",
            }),
        )
        expect(result.current.canCompleteTask).toBe(true)
        expect(result.current.completionEvidence?.outcome).toBe(
            "VERIFIED_TERMINAL",
        )
    })

    it("blocks completion when the evidence is not terminal", () => {
        const { result } = renderHook(() =>
            useSupplierOrderCenterDerivation({
                detail: makeDetail({
                    allowedActions: [
                        "OPEN_CENTER",
                        "NOTE",
                        "CONFIRM_VERIFIED_TERMINAL_RESULT",
                    ],
                    lastInvestigation: makeEvidence({
                        outcome: "VERIFIED_NO_RESULT",
                        verifiedSupplierActionResultId: undefined,
                        verifiedResolution: undefined,
                    }),
                }),
                currentUserId: "u1",
            }),
        )
        expect(result.current.canCompleteTask).toBe(false)
    })

    it("blocks completion when the action is not allowed", () => {
        const { result } = renderHook(() =>
            useSupplierOrderCenterDerivation({
                detail: makeDetail({ lastInvestigation: makeEvidence() }),
                currentUserId: "u1",
            }),
        )
        expect(result.current.canCompleteTask).toBe(false)
    })

    it("prefers a fresh investigation over the pending one", () => {
        const detail = makeDetail()
        const pending = makeEvidence({ summary: "pending" })
        const { result } = renderHook(() =>
            useSupplierOrderCenterDerivation({
                detail,
                currentUserId: "u1",
                latestInvestigation: pending,
            }),
        )
        expect(result.current.completionEvidence).toEqual(pending)
    })

    it("returns safe empty values while the detail is loading", () => {
        const { result } = renderHook(() =>
            useSupplierOrderCenterDerivation({
                detail: undefined,
                currentUserId: "u1",
            }),
        )
        expect(result.current.responsibilityStatus).toBe("blocked")
        expect(result.current.canCompleteTask).toBe(false)
        expect(result.current.canQuery).toBe(false)
        expect(result.current.canReplay).toBe(false)
        expect(result.current.canReveal).toBe(false)
        expect(result.current.isResultUnknown).toBe(false)
        expect(result.current.noQueryCapability).toBe(false)
        expect(result.current.totalQuantity).toBe(0)
        expect(result.current.totalCostGross).toBeNull()
    })
})
