import { describe, expect, it } from "vitest"

import {
    isPurchaseChangeOrderWorkItem,
    mapPurchaseChangeOrderApproval,
    mergePurchaseChangeOrderAllowedActions,
    PURCHASE_CHANGE_ORDER_DOCUMENT_TYPE,
    purchaseChangeOrderApprovalPhase,
    purchaseChangeOrderStatusLabel,
    purchaseChangeOrderStatusTone,
    readPurchaseChangeOrderApprovalResponsibility,
} from "./purchase-change-order-approval"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"

const binding: DocumentApprovalView = {
    requirement: "PROCESS_REQUIRED",
    definition: {
        id: "def-pco-1",
        name: "采购变更审批",
        version: 2,
        nodes: [
            { key: "n1", name: "仓配影响确认", assigneeName: "张三" },
            { key: "n2", name: "财务复核", assigneeName: "李四" },
        ],
        publishedNodes: [],
    },
    recentHistory: [],
    historyHasMore: false,
    allowedActions: ["SUBMIT"],
}

describe("PURCHASE_CHANGE_ORDER_DOCUMENT_TYPE", () => {
    it("uses the contract document type and does not alias PurchaseOrder", () => {
        expect(PURCHASE_CHANGE_ORDER_DOCUMENT_TYPE).toBe("PurchaseChangeOrder")
        expect(PURCHASE_CHANGE_ORDER_DOCUMENT_TYPE).not.toBe("PurchaseOrder")
        expect(PURCHASE_CHANGE_ORDER_DOCUMENT_TYPE).not.toBe("SalesChangeOrder")
    })
})

describe("purchaseChangeOrderStatusLabel", () => {
    it("maps server codes to Chinese and never prints enum leftovers", () => {
        expect(purchaseChangeOrderStatusLabel("DRAFT")).toBe("草稿")
        expect(purchaseChangeOrderStatusLabel("IN_APPROVAL")).toBe("审批中")
        expect(purchaseChangeOrderStatusLabel("PENDING_WAREHOUSE_IMPACT")).toBe(
            "审批中",
        )
        expect(purchaseChangeOrderStatusLabel("PENDING_FINANCE_REVIEW")).toBe(
            "审批中",
        )
        expect(purchaseChangeOrderStatusLabel("EFFECTIVE")).toBe("已生效")
        expect(purchaseChangeOrderStatusLabel("VOIDED")).toBe("已作废")
        expect(purchaseChangeOrderStatusLabel("UNKNOWN")).toBe("改单中")
        expect(purchaseChangeOrderStatusTone("IN_APPROVAL")).toBe("warning")
        expect(purchaseChangeOrderStatusTone("EFFECTIVE")).toBe("success")
    })
})

describe("purchaseChangeOrderApprovalPhase", () => {
    it("keeps unsubmitted documents on the binding card", () => {
        expect(purchaseChangeOrderApprovalPhase(binding, "DRAFT")).toBe("draft")
        expect(purchaseChangeOrderApprovalPhase(undefined, undefined)).toBe(
            "draft",
        )
    })

    it("switches to runtime when an instance exists", () => {
        expect(
            purchaseChangeOrderApprovalPhase(
                {
                    ...binding,
                    instance: {
                        id: "inst-pco-1",
                        status: "RUNNING",
                        currentRoundNo: 1,
                    },
                },
                "DRAFT",
            ),
        ).toBe("runtime")
        expect(purchaseChangeOrderApprovalPhase(binding, "IN_APPROVAL")).toBe(
            "runtime",
        )
    })
})

describe("mapPurchaseChangeOrderApproval", () => {
    it("maps the created binding without turning it into a work item", () => {
        const view = mapPurchaseChangeOrderApproval({
            requirement: "PROCESS_REQUIRED",
            definition: {
                id: "def-pco-1",
                name: "采购变更审批",
                version: 2,
                nodes: [
                    { key: "n1", name: "仓配影响确认", assignee_name: "张三" },
                    { key: "n2", name: "财务复核", assignee_name: "李四" },
                ],
            },
            instance: null,
            recent_history: [],
            allowed_actions: ["SUBMIT", "UPGRADE_BINDING"],
        })
        expect(view?.instance).toBeUndefined()
        expect(view?.definition?.name).toBe("采购变更审批")
        expect(view?.definition?.nodes[1]?.assigneeName).toBe("李四")
        expect(view?.allowedActions).toEqual(["SUBMIT", "UPGRADE_BINDING"])
    })

    it("returns undefined when the document has no approval projection", () => {
        expect(mapPurchaseChangeOrderApproval(null)).toBeUndefined()
        expect(mapPurchaseChangeOrderApproval(undefined)).toBeUndefined()
    })
})

describe("mergePurchaseChangeOrderAllowedActions", () => {
    it("unions server facts and drops start-processing or pool actions", () => {
        expect(
            mergePurchaseChangeOrderAllowedActions(
                ["CANCEL"],
                ["APPROVE", "START_PROCESSING", "RELEASE_TO_TEAM"],
            ),
        ).toEqual(["CANCEL", "APPROVE"])
    })
})

describe("readPurchaseChangeOrderApprovalResponsibility", () => {
    it("reads only instance fields and does not fall back to impact path or the first node", () => {
        expect(
            readPurchaseChangeOrderApprovalResponsibility({
                ...binding,
                instance: {
                    id: "inst-pco-1",
                    status: "RUNNING",
                    currentRoundNo: 2,
                    currentNodeName: "财务复核",
                    currentAssigneeName: "李四",
                },
            }),
        ).toEqual({
            nextResponsible: "李四",
            currentNodeLabel: "财务复核",
        })
        expect(readPurchaseChangeOrderApprovalResponsibility(binding)).toEqual({
            nextResponsible: undefined,
            currentNodeLabel: undefined,
        })
    })
})

describe("isPurchaseChangeOrderWorkItem", () => {
    it("accepts only the contract and object-type literals", () => {
        expect(
            isPurchaseChangeOrderWorkItem({
                businessObjectType: "PurchaseChangeOrder",
            }),
        ).toBe(true)
        expect(
            isPurchaseChangeOrderWorkItem({
                businessObjectType: "purchase_change_order",
            }),
        ).toBe(true)
        expect(
            isPurchaseChangeOrderWorkItem({
                businessObjectType: "purchase_order",
            }),
        ).toBe(false)
        expect(isPurchaseChangeOrderWorkItem(undefined)).toBe(false)
    })
})
