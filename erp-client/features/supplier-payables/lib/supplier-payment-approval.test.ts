import { describe, expect, it } from "vitest"

import {
    buildSupplierPaymentSubmitRequest,
    SUPPLIER_PAYMENT_DOCUMENT_TYPE,
    isSupplierPaymentWorkItem,
    mapSupplierPaymentApproval,
    mergeSupplierPaymentAllowedActions,
    readSupplierPaymentApprovalResponsibility,
    supplierPaymentApprovalPhase,
    supplierPaymentStatusLabel,
    supplierPaymentStatusTone,
} from "./supplier-payment-approval"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"

const binding: DocumentApprovalView = {
    requirement: "PROCESS_REQUIRED",
    definition: {
        id: "def-sp-1",
        name: "供应商付款审批",
        version: 2,
        nodes: [
            { key: "n1", name: "付款复核", assigneeName: "张三" },
            { key: "n2", name: "财务确认", assigneeName: "李四" },
        ],
        publishedNodes: [],
    },
    recentHistory: [],
    historyHasMore: false,
    allowedActions: ["SUBMIT"],
}

describe("SUPPLIER_PAYMENT_DOCUMENT_TYPE", () => {
    it("uses the contract document type and does not alias refund or reversal", () => {
        expect(SUPPLIER_PAYMENT_DOCUMENT_TYPE).toBe("SupplierPayment")
        expect(SUPPLIER_PAYMENT_DOCUMENT_TYPE).not.toBe("SupplierRefund")
        expect(SUPPLIER_PAYMENT_DOCUMENT_TYPE).not.toBe("PaymentReversal")
    })
})

describe("supplierPaymentStatusLabel", () => {
    it("maps server codes to Chinese and never prints enum leftovers", () => {
        expect(supplierPaymentStatusLabel("DRAFT")).toBe("草稿")
        expect(supplierPaymentStatusLabel("IN_APPROVAL")).toBe("审批中")
        expect(supplierPaymentStatusLabel("pending_review")).toBe("审批中")
        expect(supplierPaymentStatusLabel("POSTED")).toBe("已过账")
        expect(supplierPaymentStatusLabel("REVERSED")).toBe("已冲正")
        expect(supplierPaymentStatusLabel("UNKNOWN")).toBe("付款单")
        expect(supplierPaymentStatusTone("IN_APPROVAL")).toBe("warning")
        expect(supplierPaymentStatusTone("POSTED")).toBe("success")
    })
})

describe("supplierPaymentApprovalPhase", () => {
    it("keeps unsubmitted documents on the binding card", () => {
        expect(supplierPaymentApprovalPhase(binding, "DRAFT")).toBe("draft")
        expect(supplierPaymentApprovalPhase(undefined, undefined)).toBe("draft")
    })

    it("switches to runtime when an instance exists", () => {
        expect(
            supplierPaymentApprovalPhase(
                {
                    ...binding,
                    instance: {
                        id: "inst-sp-1",
                        status: "RUNNING",
                        currentRoundNo: 1,
                    },
                },
                "DRAFT",
            ),
        ).toBe("runtime")
        expect(supplierPaymentApprovalPhase(binding, "IN_APPROVAL")).toBe(
            "runtime",
        )
    })
})

describe("mapSupplierPaymentApproval", () => {
    it("maps the created binding without turning it into a work item", () => {
        const view = mapSupplierPaymentApproval({
            requirement: "PROCESS_REQUIRED",
            definition: {
                id: "def-sp-1",
                name: "供应商付款审批",
                version: 2,
                nodes: [
                    { key: "n1", name: "付款复核", assignee_name: "张三" },
                    { key: "n2", name: "财务确认", assignee_name: "李四" },
                ],
            },
            instance: null,
            recent_history: [],
            allowed_actions: ["SUBMIT", "UPGRADE_BINDING"],
        })
        expect(view?.instance).toBeUndefined()
        expect(view?.definition?.name).toBe("供应商付款审批")
        expect(view?.definition?.nodes[1]?.assigneeName).toBe("李四")
        expect(view?.allowedActions).toEqual(["SUBMIT", "UPGRADE_BINDING"])
    })

    it("returns undefined when the document has no approval projection", () => {
        expect(mapSupplierPaymentApproval(null)).toBeUndefined()
        expect(mapSupplierPaymentApproval(undefined)).toBeUndefined()
    })
})

describe("mergeSupplierPaymentAllowedActions", () => {
    it("unions server facts and drops generic WorkItem actions", () => {
        expect(
            mergeSupplierPaymentAllowedActions(
                ["CANCEL"],
                ["APPROVE", "REASSIGN", "CLOSE"],
            ),
        ).toEqual(["CANCEL", "APPROVE"])
    })
})

describe("readSupplierPaymentApprovalResponsibility", () => {
    it("reads only instance fields and does not fall back to review path or the first node", () => {
        expect(
            readSupplierPaymentApprovalResponsibility({
                ...binding,
                instance: {
                    id: "inst-sp-1",
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
        expect(readSupplierPaymentApprovalResponsibility(binding)).toEqual({
            nextResponsible: undefined,
            currentNodeLabel: undefined,
        })
    })
})

describe("isSupplierPaymentWorkItem", () => {
    it("accepts only the contract and object-type literals", () => {
        expect(
            isSupplierPaymentWorkItem({
                businessObjectType: "SupplierPayment",
            }),
        ).toBe(true)
        expect(
            isSupplierPaymentWorkItem({
                businessObjectType: "supplier_payment",
            }),
        ).toBe(true)
        expect(
            isSupplierPaymentWorkItem({
                businessObjectType: "SupplierRefund",
            }),
        ).toBe(false)
        expect(isSupplierPaymentWorkItem(undefined)).toBe(false)
    })
})

describe("buildSupplierPaymentSubmitRequest", () => {
    it("only emits version, idempotency key and frozen allocations", () => {
        expect(
            buildSupplierPaymentSubmitRequest({
                expectedVersion: 3,
                idempotencyKey: "k-sp-1",
                allocations: [
                    {
                        payableEntryId: "pe-1",
                        allocatedAmount: "40.00",
                    },
                ],
            }),
        ).toEqual({
            expected_version: 3,
            idempotency_key: "k-sp-1",
            allocations: [
                {
                    payable_entry_id: "pe-1",
                    allocated_amount: "40.00",
                },
            ],
        })
    })
})
