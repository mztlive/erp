import { describe, expect, it } from "vitest"

import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import type { SalesOrderListItem } from "@/features/sales-orders/types"

import type { SalesOrderDetailView } from "@/features/sales-orders/api/contracts"

import {
    canRegisterCustomerAcceptance,
    isSalesOrderApprovalInProgress,
    navItemsFor,
    resolveFocusTask,
    resolveNavSection,
} from "./sales-order-detail-model"

function approval(instanceStatus?: string): DocumentApprovalView {
    return {
        requirement: "REQUIRED",
        recentHistory: [],
        historyHasMore: false,
        allowedActions: [],
        instance:
            instanceStatus == null
                ? undefined
                : {
                      id: "inst-1",
                      status: instanceStatus,
                      currentRoundNo: 1,
                  },
    }
}

function order(input: {
    nature?: SalesOrderListItem["nature"]
    statusCode: string
    instanceStatus?: string
    hasApproval?: boolean
    allowedActions?: SalesOrderListItem["allowedActions"]
}): SalesOrderListItem {
    return {
        nature: input.nature ?? "physical_service",
        primaryStatus: {
            code: input.statusCode,
            label: input.statusCode,
            tone: "info",
        },
        approval:
            input.hasApproval === false
                ? undefined
                : approval(input.instanceStatus),
        allowedActions: input.allowedActions ?? [],
    } as SalesOrderListItem
}

describe("isSalesOrderApprovalInProgress", () => {
    it("does not treat an approved instance as pending", () => {
        expect(
            isSalesOrderApprovalInProgress(
                order({ statusCode: "effective", instanceStatus: "APPROVED" }),
            ),
        ).toBe(false)
    })

    it("treats a running instance as pending even after the document is effective", () => {
        expect(
            isSalesOrderApprovalInProgress(
                order({ statusCode: "effective", instanceStatus: "RUNNING" }),
            ),
        ).toBe(true)
    })

    it("treats a blocked instance as pending", () => {
        expect(
            isSalesOrderApprovalInProgress(
                order({ statusCode: "in_approval", instanceStatus: "BLOCKED" }),
            ),
        ).toBe(true)
    })

    it("keeps the review-stage fallback only when no instance exists yet", () => {
        expect(
            isSalesOrderApprovalInProgress(
                order({ statusCode: "in_approval" }),
            ),
        ).toBe(true)
        expect(
            isSalesOrderApprovalInProgress(
                order({
                    statusCode: "in_approval",
                    instanceStatus: "APPROVED",
                }),
            ),
        ).toBe(false)
        expect(
            isSalesOrderApprovalInProgress(order({ statusCode: "effective" })),
        ).toBe(false)
    })

    it("ignores cancelled instances", () => {
        expect(
            isSalesOrderApprovalInProgress(
                order({ statusCode: "draft", instanceStatus: "CANCELLED" }),
            ),
        ).toBe(false)
    })
})

describe("resolveFocusTask", () => {
    it("does not keep the approval banner after the instance has passed", () => {
        expect(
            resolveFocusTask(
                order({ statusCode: "effective", instanceStatus: "APPROVED" }),
                false,
            ),
        ).toBeNull()
    })

    it("keeps the approval banner while the instance is still running", () => {
        expect(
            resolveFocusTask(
                order({
                    statusCode: "in_approval",
                    instanceStatus: "RUNNING",
                }),
                true,
            ),
        ).toMatchObject({
            id: "approval",
            title: "销售单等审批",
        })
    })

    it("falls through to acceptance once approval has finished and facts exist", () => {
        expect(
            resolveFocusTask(
                order({ statusCode: "effective", instanceStatus: "APPROVED" }),
                true,
            ),
        ).toMatchObject({
            id: "acceptance",
            title: "可以做客户验收",
        })
    })

    it("does not prompt acceptance after approval when nothing has been fulfilled", () => {
        expect(
            resolveFocusTask(
                order({ statusCode: "effective", instanceStatus: "APPROVED" }),
                false,
            ),
        ).toBeNull()
    })

    it("uses the voucher copy for a running card-voucher approval", () => {
        expect(
            resolveFocusTask(
                order({
                    nature: "card_voucher",
                    statusCode: "in_approval",
                    instanceStatus: "RUNNING",
                }),
                false,
            ),
        ).toMatchObject({
            id: "approval",
            title: "卡券销售等审批",
        })
    })
})

describe("canRegisterCustomerAcceptance", () => {
    it("requires remaining eligible fulfillment facts, not just an effective order", () => {
        const physical = order({
            statusCode: "effective",
            allowedActions: ["REGISTER_ACCEPTANCE"],
        })
        expect(canRegisterCustomerAcceptance(physical, false)).toBe(false)
        expect(canRegisterCustomerAcceptance(physical, true)).toBe(true)
    })

    it("does not allow acceptance on card-voucher orders", () => {
        expect(
            canRegisterCustomerAcceptance(
                order({
                    nature: "card_voucher",
                    statusCode: "effective",
                    allowedActions: ["REGISTER_ACCEPTANCE"],
                }),
                true,
            ),
        ).toBe(false)
    })
})

describe("resolveNavSection", () => {
    it("treats acceptance as its own tab for physical orders", () => {
        expect(
            resolveNavSection("acceptance", { from: null, isCard: false }),
        ).toBe("acceptance")
        expect(
            resolveNavSection("fulfillment", { from: null, isCard: false }),
        ).toBe("fulfillment")
    })

    it("falls back to fulfillment for card-voucher acceptance urls", () => {
        expect(
            resolveNavSection("acceptance", { from: null, isCard: true }),
        ).toBe("fulfillment")
    })
})

describe("navItemsFor", () => {
    it("shows the acceptance tab only on physical-service orders", () => {
        const physical = order({
            statusCode: "effective",
        }) as SalesOrderDetailView
        const card = order({
            nature: "card_voucher",
            statusCode: "effective",
        }) as SalesOrderDetailView
        expect(
            navItemsFor(physical).find((item) => item.id === "acceptance"),
        ).toMatchObject({ show: true, label: "验收" })
        expect(
            navItemsFor(card).find((item) => item.id === "acceptance"),
        ).toMatchObject({ show: false })
    })
})
