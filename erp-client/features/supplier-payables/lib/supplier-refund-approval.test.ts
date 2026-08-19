import { readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

import { describe, expect, it } from "vitest"

import {
    buildSupplierRefundSubmitRequest,
    SUPPLIER_REFUND_APPROVAL_REQUIREMENT,
    SUPPLIER_REFUND_DOCUMENT_TYPE,
    SUPPLIER_REFUND_OBJECT_TYPE,
    isSupplierRefundWorkItem,
    mapSupplierRefundApproval,
    mergeSupplierRefundAllowedActions,
    readSupplierRefundApprovalResponsibility,
    slotForSupplierRefundIntent,
    supplierRefundApprovalPhase,
    supplierRefundIntentFingerprint,
    supplierRefundStatusLabel,
    supplierRefundStatusTone,
} from "./supplier-refund-approval"
import { SUPPLIER_PAYMENT_DOCUMENT_TYPE } from "./supplier-payment-approval"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import {
    SUPPLIER_ACCOUNTS_REFUND_APPROVAL_REQUIREMENT,
    SUPPLIER_ACCOUNTS_REFUND_FORBIDDEN_ACTIONS,
} from "@/app/(workspace)/finance/supplier-accounts/supplier-refund-page-proof"

const binding: DocumentApprovalView = {
    requirement: "PROCESS_REQUIRED",
    definition: {
        id: "def-srf-1",
        name: "供应商退款审批",
        version: 2,
        nodes: [
            { key: "n1", name: "退款复核", assigneeName: "张三" },
            { key: "n2", name: "财务确认", assigneeName: "李四" },
        ],
        publishedNodes: [],
    },
    recentHistory: [],
    historyHasMore: false,
    allowedActions: ["SUBMIT"],
}

describe("SUPPLIER_REFUND_DOCUMENT_TYPE", () => {
    it("uses the contract document type and does not alias payment", () => {
        expect(SUPPLIER_REFUND_DOCUMENT_TYPE).toBe("SupplierRefund")
        expect(SUPPLIER_REFUND_OBJECT_TYPE).toBe("supplier_refund")
        expect(SUPPLIER_REFUND_APPROVAL_REQUIREMENT).toBe("PROCESS_REQUIRED")
        expect(SUPPLIER_REFUND_DOCUMENT_TYPE).not.toBe(
            SUPPLIER_PAYMENT_DOCUMENT_TYPE,
        )
        expect(SUPPLIER_REFUND_DOCUMENT_TYPE).not.toBe("PaymentReversal")
    })
})

describe("supplierRefundStatusLabel", () => {
    it("maps server codes to Chinese and never prints enum leftovers", () => {
        expect(supplierRefundStatusLabel("DRAFT")).toBe("草稿")
        expect(supplierRefundStatusLabel("IN_APPROVAL")).toBe("审批中")
        expect(supplierRefundStatusLabel("pending_review")).toBe("审批中")
        expect(supplierRefundStatusLabel("POSTED")).toBe("已过账")
        expect(supplierRefundStatusLabel("REVERSED")).toBe("已冲正")
        expect(supplierRefundStatusLabel("UNKNOWN")).toBe("退款单")
        expect(supplierRefundStatusTone("IN_APPROVAL")).toBe("warning")
        expect(supplierRefundStatusTone("POSTED")).toBe("success")
    })
})

describe("supplierRefundApprovalPhase", () => {
    it("keeps unsubmitted documents on the binding card", () => {
        expect(supplierRefundApprovalPhase(binding, "DRAFT")).toBe("draft")
        expect(supplierRefundApprovalPhase(undefined, undefined)).toBe("draft")
    })

    it("switches to runtime when an instance exists", () => {
        expect(
            supplierRefundApprovalPhase(
                {
                    ...binding,
                    instance: {
                        id: "inst-srf-1",
                        status: "RUNNING",
                        currentRoundNo: 1,
                    },
                },
                "DRAFT",
            ),
        ).toBe("runtime")
        expect(supplierRefundApprovalPhase(binding, "IN_APPROVAL")).toBe(
            "runtime",
        )
    })
})

describe("mapSupplierRefundApproval", () => {
    it("maps the created binding without turning it into a work item", () => {
        const view = mapSupplierRefundApproval({
            requirement: "PROCESS_REQUIRED",
            definition: {
                id: "def-srf-1",
                name: "供应商退款审批",
                version: 2,
                nodes: [
                    { key: "n1", name: "退款复核", assignee_name: "张三" },
                    { key: "n2", name: "财务确认", assignee_name: "李四" },
                ],
            },
            instance: null,
            recent_history: [],
            allowed_actions: ["SUBMIT", "UPGRADE_BINDING"],
        })
        expect(view?.instance).toBeUndefined()
        expect(view?.definition?.name).toBe("供应商退款审批")
        expect(view?.definition?.nodes[1]?.assigneeName).toBe("李四")
        expect(view?.allowedActions).toEqual(["SUBMIT", "UPGRADE_BINDING"])
    })

    it("returns undefined when the document has no approval projection", () => {
        expect(mapSupplierRefundApproval(null)).toBeUndefined()
        expect(mapSupplierRefundApproval(undefined)).toBeUndefined()
    })
})

describe("mergeSupplierRefundAllowedActions", () => {
    it("unions server facts and drops start-processing or pool actions", () => {
        expect(
            mergeSupplierRefundAllowedActions(
                ["CANCEL"],
                ["APPROVE", "START_PROCESSING", "RELEASE_TO_TEAM"],
            ),
        ).toEqual(["CANCEL", "APPROVE"])
    })
})

describe("readSupplierRefundApprovalResponsibility", () => {
    it("reads only instance fields and does not fall back to review path or the first node", () => {
        expect(
            readSupplierRefundApprovalResponsibility({
                ...binding,
                instance: {
                    id: "inst-srf-1",
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
        expect(readSupplierRefundApprovalResponsibility(binding)).toEqual({
            nextResponsible: undefined,
            currentNodeLabel: undefined,
        })
    })
})

describe("isSupplierRefundWorkItem", () => {
    it("accepts only the contract and object-type literals", () => {
        expect(
            isSupplierRefundWorkItem({
                businessObjectType: "SupplierRefund",
            }),
        ).toBe(true)
        expect(
            isSupplierRefundWorkItem({
                businessObjectType: "supplier_refund",
            }),
        ).toBe(true)
        expect(
            isSupplierRefundWorkItem({
                businessObjectType: "SupplierPayment",
            }),
        ).toBe(false)
        expect(isSupplierRefundWorkItem(undefined)).toBe(false)
    })
})

describe("buildSupplierRefundSubmitRequest", () => {
    it("only emits version and idempotency key", () => {
        expect(
            buildSupplierRefundSubmitRequest({
                expectedVersion: 3,
                idempotencyKey: "k-srf-1",
            }),
        ).toEqual({
            expected_version: 3,
            idempotency_key: "k-srf-1",
        })
    })
})

describe("slotForSupplierRefundIntent", () => {
    it("reuses the key for the same source and reason", () => {
        const first = slotForSupplierRefundIntent(null, "src-a", "退差额")
        const retry = slotForSupplierRefundIntent(first, "src-a", "退差额")
        expect(retry.key).toBe(first.key)
        expect(retry.fingerprint).toBe(
            supplierRefundIntentFingerprint("src-a", "退差额"),
        )
    })

    it("rotates when the source payment or reason changes", () => {
        const first = slotForSupplierRefundIntent(null, "src-a", "退差额")
        const otherSource = slotForSupplierRefundIntent(
            first,
            "src-b",
            "退差额",
        )
        const otherReason = slotForSupplierRefundIntent(
            otherSource,
            "src-b",
            "全额退",
        )
        expect(otherSource.key).not.toBe(first.key)
        expect(otherReason.key).not.toBe(otherSource.key)
        expect(otherSource.key.startsWith("w12-rev-src-b-")).toBe(true)
    })
})

describe("supplier accounts page refund proof", () => {
    it("declares PROCESS_REQUIRED and keeps invoice on the no-approval path", () => {
        expect(SUPPLIER_ACCOUNTS_REFUND_APPROVAL_REQUIREMENT).toBe(
            "PROCESS_REQUIRED",
        )
        expect(SUPPLIER_ACCOUNTS_REFUND_FORBIDDEN_ACTIONS).toEqual(
            expect.arrayContaining(["选择流程", "开始处理", "退回团队"]),
        )
        const pagePath = join(
            dirname(fileURLToPath(import.meta.url)),
            "../../../app/(workspace)/finance/supplier-accounts/page.tsx",
        )
        const pageSource = readFileSync(pagePath, "utf8")
        expect(pageSource).toContain("SupplierRefund 为 PROCESS_REQUIRED")
        expect(pageSource).toContain("Invoice 为 NO_APPROVAL")
        for (const label of SUPPLIER_ACCOUNTS_REFUND_FORBIDDEN_ACTIONS) {
            expect(pageSource).not.toContain(label)
        }
    })
})
