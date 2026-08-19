import { describe, expect, it } from "vitest"

import {
    mapPurchaseOrderApproval,
    mergePurchaseOrderAllowedActions,
    purchaseOrderApprovalPhase,
    purchaseOrderStatusLabel,
    PURCHASE_ORDER_DOCUMENT_TYPE,
    readPurchaseOrderApprovalResponsibility,
} from "./purchase-order-approval"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"

const binding: DocumentApprovalView = {
    requirement: "PROCESS_REQUIRED",
    definition: {
        id: "def-po-1",
        name: "采购单审批",
        version: 2,
        nodes: [
            { key: "n1", name: "采购审核", assigneeName: "张三" },
            { key: "n2", name: "财务复核", assigneeName: "李四" },
        ],
        publishedNodes: [],
    },
    recentHistory: [],
    historyHasMore: false,
    allowedActions: ["SUBMIT"],
}

describe("PURCHASE_ORDER_DOCUMENT_TYPE", () => {
    it("uses the contract document type and does not alias change orders", () => {
        expect(PURCHASE_ORDER_DOCUMENT_TYPE).toBe("PurchaseOrder")
        expect(PURCHASE_ORDER_DOCUMENT_TYPE).not.toBe("PurchaseChangeOrder")
        expect(PURCHASE_ORDER_DOCUMENT_TYPE).not.toBe("SalesOrder")
    })
})

describe("purchaseOrderStatusLabel", () => {
    it("maps server codes to Chinese and never prints enum leftovers", () => {
        expect(purchaseOrderStatusLabel("DRAFT")).toBe("草稿")
        expect(purchaseOrderStatusLabel("IN_APPROVAL")).toBe("审批中")
        expect(purchaseOrderStatusLabel("PENDING_FINANCE_REVIEW")).toBe(
            "审批中",
        )
        expect(purchaseOrderStatusLabel("PENDING_REVIEW")).toBe("审批中")
        expect(purchaseOrderStatusLabel("EFFECTIVE")).toBe("已生效")
        expect(purchaseOrderStatusLabel("VOIDED")).toBe("已作废")
        expect(purchaseOrderStatusLabel("UNKNOWN")).toBe("采购单")
    })
})

describe("purchaseOrderApprovalPhase", () => {
    it("keeps unsubmitted documents on the binding card", () => {
        expect(purchaseOrderApprovalPhase(binding, "DRAFT")).toBe("draft")
        expect(purchaseOrderApprovalPhase(undefined, undefined)).toBe("draft")
    })

    it("switches to runtime when an instance exists", () => {
        expect(
            purchaseOrderApprovalPhase(
                {
                    ...binding,
                    instance: {
                        id: "inst-po-1",
                        status: "RUNNING",
                        currentRoundNo: 1,
                    },
                },
                "DRAFT",
            ),
        ).toBe("runtime")
        expect(purchaseOrderApprovalPhase(binding, "IN_APPROVAL")).toBe(
            "runtime",
        )
        expect(purchaseOrderApprovalPhase(binding, "PENDING_REVIEW")).toBe(
            "runtime",
        )
    })
})

describe("mapPurchaseOrderApproval", () => {
    it("maps the created binding without turning it into a work item", () => {
        const view = mapPurchaseOrderApproval({
            requirement: "PROCESS_REQUIRED",
            definition: {
                id: "def-po-1",
                name: "采购单审批",
                version: 2,
                nodes: [
                    { key: "n1", name: "采购审核", assignee_name: "张三" },
                    { key: "n2", name: "财务复核", assignee_name: "李四" },
                ],
            },
            instance: null,
            recent_history: [],
            allowed_actions: ["SUBMIT", "UPGRADE_BINDING"],
        })
        expect(view?.instance).toBeUndefined()
        expect(view?.definition?.name).toBe("采购单审批")
        expect(view?.definition?.nodes[1]?.assigneeName).toBe("李四")
        expect(view?.allowedActions).toEqual(["SUBMIT", "UPGRADE_BINDING"])
    })

    it("returns undefined when the document has no approval projection", () => {
        expect(mapPurchaseOrderApproval(null)).toBeUndefined()
        expect(mapPurchaseOrderApproval(undefined)).toBeUndefined()
    })
})

describe("mergePurchaseOrderAllowedActions", () => {
    it("unions server facts and drops start-processing or pool actions", () => {
        expect(
            mergePurchaseOrderAllowedActions(
                ["CANCEL"],
                ["APPROVE", "START_PROCESSING", "RELEASE_TO_TEAM"],
            ),
        ).toEqual(["CANCEL", "APPROVE"])
    })
})

describe("readPurchaseOrderApprovalResponsibility", () => {
    it("reads only instance fields and does not fall back to the first node", () => {
        expect(
            readPurchaseOrderApprovalResponsibility({
                ...binding,
                instance: {
                    id: "inst-po-1",
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
        expect(readPurchaseOrderApprovalResponsibility(binding)).toEqual({
            nextResponsible: undefined,
            currentNodeLabel: undefined,
        })
    })
})
