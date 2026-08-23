import { describe, expect, it } from "vitest"

import {
    mapVoucherSalesOrderApproval,
    mergeVoucherSalesOrderAllowedActions,
    readVoucherSalesOrderApprovalResponsibility,
    VOUCHER_SALES_ORDER_DOCUMENT_TYPE,
    voucherSalesOrderApprovalPhase,
} from "./voucher-sales-order-approval"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"

const binding: DocumentApprovalView = {
    requirement: "PROCESS_REQUIRED",
    definition: {
        id: "def-v1",
        name: "卡券销售审批",
        version: 3,
        nodes: [
            { key: "n1", name: "销售审核", assigneeName: "张三" },
            { key: "n2", name: "卡券运营", assigneeName: "李四" },
        ],
        publishedNodes: [],
    },
    recentHistory: [],
    historyHasMore: false,
    allowedActions: ["SUBMIT"],
}

describe("VOUCHER_SALES_ORDER_DOCUMENT_TYPE", () => {
    it("uses the contract document type and does not alias SalesOrder", () => {
        expect(VOUCHER_SALES_ORDER_DOCUMENT_TYPE).toBe("VoucherSalesOrder")
        expect(VOUCHER_SALES_ORDER_DOCUMENT_TYPE).not.toBe("SalesOrder")
    })
})

describe("voucherSalesOrderApprovalPhase", () => {
    it("keeps unsubmitted documents on the binding card", () => {
        expect(voucherSalesOrderApprovalPhase(binding, "draft")).toBe("draft")
        expect(voucherSalesOrderApprovalPhase(undefined, undefined)).toBe(
            "draft",
        )
    })

    it("switches to runtime when an instance exists", () => {
        expect(
            voucherSalesOrderApprovalPhase(
                {
                    ...binding,
                    instance: {
                        id: "inst-v1",
                        status: "RUNNING",
                        currentRoundNo: 1,
                    },
                },
                "draft",
            ),
        ).toBe("runtime")
        expect(voucherSalesOrderApprovalPhase(binding, "in_approval")).toBe(
            "runtime",
        )
    })
})

describe("mapVoucherSalesOrderApproval", () => {
    it("maps the created binding without turning it into a work item", () => {
        const view = mapVoucherSalesOrderApproval({
            requirement: "PROCESS_REQUIRED",
            definition: {
                id: "def-v1",
                name: "卡券销售审批",
                version: 3,
                nodes: [
                    { key: "n1", name: "销售审核", assignee_name: "张三" },
                    { key: "n2", name: "卡券运营", assignee_name: "李四" },
                ],
            },
            instance: null,
            recent_history: [],
            allowed_actions: ["SUBMIT", "UPGRADE_BINDING"],
        })
        expect(view?.instance).toBeUndefined()
        expect(view?.definition?.name).toBe("卡券销售审批")
        expect(view?.definition?.nodes[1]?.assigneeName).toBe("李四")
        expect(view?.allowedActions).toEqual(["SUBMIT", "UPGRADE_BINDING"])
    })

    it("returns undefined when the document has no approval projection", () => {
        expect(mapVoucherSalesOrderApproval(null)).toBeUndefined()
        expect(mapVoucherSalesOrderApproval(undefined)).toBeUndefined()
    })
})

describe("mergeVoucherSalesOrderAllowedActions", () => {
    it("unions server facts and drops generic WorkItem actions", () => {
        expect(
            mergeVoucherSalesOrderAllowedActions(
                ["CANCEL"],
                ["APPROVE", "REASSIGN", "CLOSE"],
            ),
        ).toEqual(["CANCEL", "APPROVE"])
    })
})

describe("readVoucherSalesOrderApprovalResponsibility", () => {
    it("reads only instance fields and does not fall back to operations or the first node", () => {
        expect(
            readVoucherSalesOrderApprovalResponsibility({
                ...binding,
                instance: {
                    id: "inst-v1",
                    status: "RUNNING",
                    currentRoundNo: 2,
                    currentNodeName: "卡券运营",
                    currentAssigneeName: "李四",
                },
            }),
        ).toEqual({
            nextResponsible: "李四",
            currentNodeLabel: "卡券运营",
        })
        expect(readVoucherSalesOrderApprovalResponsibility(binding)).toEqual({
            nextResponsible: undefined,
            currentNodeLabel: undefined,
        })
    })
})
