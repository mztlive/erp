import { describe, expect, it } from "vitest"

import {
    mapSalesChangeOrderApproval,
    mergeSalesChangeOrderAllowedActions,
    readSalesChangeOrderApprovalResponsibility,
    SALES_CHANGE_ORDER_DOCUMENT_TYPE,
    salesChangeOrderApprovalPhase,
    salesChangeOrderStatusLabel,
} from "./sales-change-order-approval"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"

const binding: DocumentApprovalView = {
    requirement: "PROCESS_REQUIRED",
    definition: {
        id: "def-sc-1",
        name: "销售变更审批",
        version: 2,
        nodes: [
            { key: "n1", name: "履约影响确认", assigneeName: "张三" },
            { key: "n2", name: "财务复核", assigneeName: "李四" },
        ],
        publishedNodes: [],
    },
    recentHistory: [],
    historyHasMore: false,
    allowedActions: ["SUBMIT"],
}

describe("SALES_CHANGE_ORDER_DOCUMENT_TYPE", () => {
    it("uses the contract document type and does not alias SalesOrder", () => {
        expect(SALES_CHANGE_ORDER_DOCUMENT_TYPE).toBe("SalesChangeOrder")
        expect(SALES_CHANGE_ORDER_DOCUMENT_TYPE).not.toBe("SalesOrder")
        expect(SALES_CHANGE_ORDER_DOCUMENT_TYPE).not.toBe("VoucherSalesOrder")
    })
})

describe("salesChangeOrderStatusLabel", () => {
    it("maps server codes to Chinese and never prints enum leftovers", () => {
        expect(salesChangeOrderStatusLabel("DRAFT")).toBe("草稿")
        expect(salesChangeOrderStatusLabel("IN_APPROVAL")).toBe("审批中")
        expect(salesChangeOrderStatusLabel("PENDING_IMPACT_CONFIRMATION")).toBe(
            "审批中",
        )
        expect(salesChangeOrderStatusLabel("PENDING_FINANCE_REVIEW")).toBe(
            "审批中",
        )
        expect(salesChangeOrderStatusLabel("EFFECTIVE")).toBe("已生效")
        expect(salesChangeOrderStatusLabel("VOIDED")).toBe("已作废")
        expect(salesChangeOrderStatusLabel("UNKNOWN")).toBe("改单中")
    })
})

describe("salesChangeOrderApprovalPhase", () => {
    it("keeps unsubmitted documents on the binding card", () => {
        expect(salesChangeOrderApprovalPhase(binding, "DRAFT")).toBe("draft")
        expect(salesChangeOrderApprovalPhase(undefined, undefined)).toBe(
            "draft",
        )
    })

    it("switches to runtime when an instance exists", () => {
        expect(
            salesChangeOrderApprovalPhase(
                {
                    ...binding,
                    instance: {
                        id: "inst-sc-1",
                        status: "RUNNING",
                        currentRoundNo: 1,
                    },
                },
                "DRAFT",
            ),
        ).toBe("runtime")
        expect(salesChangeOrderApprovalPhase(binding, "IN_APPROVAL")).toBe(
            "runtime",
        )
    })
})

describe("mapSalesChangeOrderApproval", () => {
    it("maps the created binding without turning it into a work item", () => {
        const view = mapSalesChangeOrderApproval({
            requirement: "PROCESS_REQUIRED",
            definition: {
                id: "def-sc-1",
                name: "销售变更审批",
                version: 2,
                nodes: [
                    { key: "n1", name: "履约影响确认", assignee_name: "张三" },
                    { key: "n2", name: "财务复核", assignee_name: "李四" },
                ],
            },
            instance: null,
            recent_history: [],
            allowed_actions: ["SUBMIT", "UPGRADE_BINDING"],
        })
        expect(view?.instance).toBeUndefined()
        expect(view?.definition?.name).toBe("销售变更审批")
        expect(view?.definition?.nodes[1]?.assigneeName).toBe("李四")
        expect(view?.allowedActions).toEqual(["SUBMIT", "UPGRADE_BINDING"])
    })

    it("returns undefined when the document has no approval projection", () => {
        expect(mapSalesChangeOrderApproval(null)).toBeUndefined()
        expect(mapSalesChangeOrderApproval(undefined)).toBeUndefined()
    })
})

describe("mergeSalesChangeOrderAllowedActions", () => {
    it("unions server facts and drops generic WorkItem actions", () => {
        expect(
            mergeSalesChangeOrderAllowedActions(
                ["CANCEL"],
                ["APPROVE", "REASSIGN", "CLOSE"],
            ),
        ).toEqual(["CANCEL", "APPROVE"])
    })
})

describe("readSalesChangeOrderApprovalResponsibility", () => {
    it("reads only instance fields and does not fall back to impact path or the first node", () => {
        expect(
            readSalesChangeOrderApprovalResponsibility({
                ...binding,
                instance: {
                    id: "inst-sc-1",
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
        expect(readSalesChangeOrderApprovalResponsibility(binding)).toEqual({
            nextResponsible: undefined,
            currentNodeLabel: undefined,
        })
    })
})
