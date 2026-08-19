import { describe, expect, it } from "vitest"

import {
    buildCustomerReceiptSubmitRequest,
    CUSTOMER_RECEIPT_DOCUMENT_TYPE,
    customerReceiptApprovalPhase,
    customerReceiptStatusLabel,
    customerReceiptStatusTone,
    isCustomerReceiptWorkItem,
    mapCustomerReceiptApproval,
    mergeCustomerReceiptAllowedActions,
    readCustomerReceiptApprovalResponsibility,
} from "./customer-receipt-approval"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"

const binding: DocumentApprovalView = {
    requirement: "PROCESS_REQUIRED",
    definition: {
        id: "def-cr-1",
        name: "客户回款审批",
        version: 2,
        nodes: [
            { key: "n1", name: "回款复核", assigneeName: "张三" },
            { key: "n2", name: "财务确认", assigneeName: "李四" },
        ],
        publishedNodes: [],
    },
    recentHistory: [],
    historyHasMore: false,
    allowedActions: ["SUBMIT"],
}

describe("CUSTOMER_RECEIPT_DOCUMENT_TYPE", () => {
    it("uses the contract document type and does not alias invoice", () => {
        expect(CUSTOMER_RECEIPT_DOCUMENT_TYPE).toBe("CustomerReceipt")
        expect(CUSTOMER_RECEIPT_DOCUMENT_TYPE).not.toBe("Invoice")
        expect(CUSTOMER_RECEIPT_DOCUMENT_TYPE).not.toBe("CustomerRefund")
    })
})

describe("customerReceiptStatusLabel", () => {
    it("maps server codes to Chinese and never prints enum leftovers", () => {
        expect(customerReceiptStatusLabel("DRAFT")).toBe("草稿")
        expect(customerReceiptStatusLabel("IN_APPROVAL")).toBe("审批中")
        expect(customerReceiptStatusLabel("pending_review")).toBe("审批中")
        expect(customerReceiptStatusLabel("POSTED")).toBe("已过账")
        expect(customerReceiptStatusLabel("REVERSED")).toBe("已冲正")
        expect(customerReceiptStatusLabel("UNKNOWN")).toBe("回款单")
        expect(customerReceiptStatusTone("IN_APPROVAL")).toBe("warning")
        expect(customerReceiptStatusTone("POSTED")).toBe("success")
    })
})

describe("customerReceiptApprovalPhase", () => {
    it("keeps unsubmitted documents on the binding card", () => {
        expect(customerReceiptApprovalPhase(binding, "DRAFT")).toBe("draft")
        expect(customerReceiptApprovalPhase(undefined, undefined)).toBe("draft")
    })

    it("switches to runtime when an instance exists", () => {
        expect(
            customerReceiptApprovalPhase(
                {
                    ...binding,
                    instance: {
                        id: "inst-cr-1",
                        status: "RUNNING",
                        currentRoundNo: 1,
                    },
                },
                "DRAFT",
            ),
        ).toBe("runtime")
        expect(customerReceiptApprovalPhase(binding, "IN_APPROVAL")).toBe(
            "runtime",
        )
    })
})

describe("mapCustomerReceiptApproval", () => {
    it("maps the created binding without turning it into a work item", () => {
        const view = mapCustomerReceiptApproval({
            requirement: "PROCESS_REQUIRED",
            definition: {
                id: "def-cr-1",
                name: "客户回款审批",
                version: 2,
                nodes: [
                    { key: "n1", name: "回款复核", assignee_name: "张三" },
                    { key: "n2", name: "财务确认", assignee_name: "李四" },
                ],
            },
            instance: null,
            recent_history: [],
            allowed_actions: ["SUBMIT", "UPGRADE_BINDING"],
        })
        expect(view?.instance).toBeUndefined()
        expect(view?.definition?.name).toBe("客户回款审批")
        expect(view?.definition?.nodes[1]?.assigneeName).toBe("李四")
        expect(view?.allowedActions).toEqual(["SUBMIT", "UPGRADE_BINDING"])
    })

    it("returns undefined when the document has no approval projection", () => {
        expect(mapCustomerReceiptApproval(null)).toBeUndefined()
        expect(mapCustomerReceiptApproval(undefined)).toBeUndefined()
    })
})

describe("mergeCustomerReceiptAllowedActions", () => {
    it("unions server facts and drops start-processing or pool actions", () => {
        expect(
            mergeCustomerReceiptAllowedActions(
                ["CANCEL"],
                ["APPROVE", "START_PROCESSING", "RELEASE_TO_TEAM"],
            ),
        ).toEqual(["CANCEL", "APPROVE"])
    })
})

describe("readCustomerReceiptApprovalResponsibility", () => {
    it("reads only instance fields and does not fall back to review path or the first node", () => {
        expect(
            readCustomerReceiptApprovalResponsibility({
                ...binding,
                instance: {
                    id: "inst-cr-1",
                    status: "RUNNING",
                    currentRoundNo: 2,
                    currentNodeName: "财务确认",
                    currentAssigneeName: "李四",
                },
            }),
        ).toEqual({
            nextResponsible: "李四",
            currentNodeLabel: "财务确认",
        })
        expect(readCustomerReceiptApprovalResponsibility(binding)).toEqual({
            nextResponsible: undefined,
            currentNodeLabel: undefined,
        })
    })
})

describe("isCustomerReceiptWorkItem", () => {
    it("accepts only the contract and object-type literals", () => {
        expect(
            isCustomerReceiptWorkItem({
                businessObjectType: "CustomerReceipt",
            }),
        ).toBe(true)
        expect(
            isCustomerReceiptWorkItem({
                businessObjectType: "customer_receipt",
            }),
        ).toBe(true)
        expect(
            isCustomerReceiptWorkItem({
                businessObjectType: "invoice",
            }),
        ).toBe(false)
        expect(isCustomerReceiptWorkItem(undefined)).toBe(false)
    })
})

describe("buildCustomerReceiptSubmitRequest", () => {
    it("only emits version, idempotency key and frozen allocations", () => {
        expect(
            buildCustomerReceiptSubmitRequest({
                expectedVersion: 3,
                idempotencyKey: "k-cr-1",
                allocations: [
                    {
                        receivableEntryId: "re-1",
                        allocatedAmount: "40.00",
                    },
                ],
            }),
        ).toEqual({
            expected_version: 3,
            idempotency_key: "k-cr-1",
            allocations: [
                {
                    receivable_entry_id: "re-1",
                    allocated_amount: "40.00",
                },
            ],
        })
    })
})
