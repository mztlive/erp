import { readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

import { describe, expect, it } from "vitest"

import {
    buildCustomerRefundSubmitRequest,
    CUSTOMER_REFUND_APPROVAL_REQUIREMENT,
    CUSTOMER_REFUND_DOCUMENT_TYPE,
    CUSTOMER_REFUND_OBJECT_TYPE,
    customerRefundApprovalPhase,
    customerRefundStatusLabel,
    customerRefundStatusTone,
    isCustomerRefundWorkItem,
    customerRefundIntentFingerprint,
    mapCustomerRefundApproval,
    mergeCustomerRefundAllowedActions,
    readCustomerRefundApprovalResponsibility,
    slotForCustomerRefundIntent,
} from "./customer-refund-approval"
import { CUSTOMER_RECEIPT_DOCUMENT_TYPE } from "./customer-receipt-approval"
import { INVOICE_DOCUMENT_TYPE } from "./invoice-no-approval"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import {
    CUSTOMER_ACCOUNTS_REFUND_APPROVAL_REQUIREMENT,
    CUSTOMER_ACCOUNTS_REFUND_FORBIDDEN_ACTIONS,
} from "@/app/(workspace)/finance/customer-accounts/customer-refund-page-proof"

const binding: DocumentApprovalView = {
    requirement: "PROCESS_REQUIRED",
    definition: {
        id: "def-crf-1",
        name: "客户退款审批",
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

describe("CUSTOMER_REFUND_DOCUMENT_TYPE", () => {
    it("uses the contract document type and does not alias receipt or invoice", () => {
        expect(CUSTOMER_REFUND_DOCUMENT_TYPE).toBe("CustomerRefund")
        expect(CUSTOMER_REFUND_OBJECT_TYPE).toBe("customer_refund")
        expect(CUSTOMER_REFUND_APPROVAL_REQUIREMENT).toBe("PROCESS_REQUIRED")
        expect(CUSTOMER_REFUND_DOCUMENT_TYPE).not.toBe(
            CUSTOMER_RECEIPT_DOCUMENT_TYPE,
        )
        expect(CUSTOMER_REFUND_DOCUMENT_TYPE).not.toBe(INVOICE_DOCUMENT_TYPE)
    })
})

describe("customerRefundStatusLabel", () => {
    it("maps server codes to Chinese and never prints enum leftovers", () => {
        expect(customerRefundStatusLabel("DRAFT")).toBe("草稿")
        expect(customerRefundStatusLabel("IN_APPROVAL")).toBe("审批中")
        expect(customerRefundStatusLabel("pending_review")).toBe("审批中")
        expect(customerRefundStatusLabel("POSTED")).toBe("已过账")
        expect(customerRefundStatusLabel("REVERSED")).toBe("已冲正")
        expect(customerRefundStatusLabel("UNKNOWN")).toBe("退款单")
        expect(customerRefundStatusTone("IN_APPROVAL")).toBe("warning")
        expect(customerRefundStatusTone("POSTED")).toBe("success")
    })
})

describe("customerRefundApprovalPhase", () => {
    it("keeps unsubmitted documents on the binding card", () => {
        expect(customerRefundApprovalPhase(binding, "DRAFT")).toBe("draft")
        expect(customerRefundApprovalPhase(undefined, undefined)).toBe("draft")
    })

    it("switches to runtime when an instance exists", () => {
        expect(
            customerRefundApprovalPhase(
                {
                    ...binding,
                    instance: {
                        id: "inst-crf-1",
                        status: "RUNNING",
                        currentRoundNo: 1,
                    },
                },
                "DRAFT",
            ),
        ).toBe("runtime")
        expect(customerRefundApprovalPhase(binding, "IN_APPROVAL")).toBe(
            "runtime",
        )
    })
})

describe("mapCustomerRefundApproval", () => {
    it("maps the created binding without turning it into a work item", () => {
        const view = mapCustomerRefundApproval({
            requirement: "PROCESS_REQUIRED",
            definition: {
                id: "def-crf-1",
                name: "客户退款审批",
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
        expect(view?.definition?.name).toBe("客户退款审批")
        expect(view?.definition?.nodes[1]?.assigneeName).toBe("李四")
        expect(view?.allowedActions).toEqual(["SUBMIT", "UPGRADE_BINDING"])
    })

    it("returns undefined when the document has no approval projection", () => {
        expect(mapCustomerRefundApproval(null)).toBeUndefined()
        expect(mapCustomerRefundApproval(undefined)).toBeUndefined()
    })
})

describe("mergeCustomerRefundAllowedActions", () => {
    it("unions server facts and drops start-processing or pool actions", () => {
        expect(
            mergeCustomerRefundAllowedActions(
                ["CANCEL"],
                ["APPROVE", "START_PROCESSING", "RELEASE_TO_TEAM"],
            ),
        ).toEqual(["CANCEL", "APPROVE"])
    })
})

describe("readCustomerRefundApprovalResponsibility", () => {
    it("reads only instance fields and does not fall back to review path or the first node", () => {
        expect(
            readCustomerRefundApprovalResponsibility({
                ...binding,
                instance: {
                    id: "inst-crf-1",
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
        expect(readCustomerRefundApprovalResponsibility(binding)).toEqual({
            nextResponsible: undefined,
            currentNodeLabel: undefined,
        })
    })
})

describe("isCustomerRefundWorkItem", () => {
    it("accepts only the contract and object-type literals", () => {
        expect(
            isCustomerRefundWorkItem({
                businessObjectType: "CustomerRefund",
            }),
        ).toBe(true)
        expect(
            isCustomerRefundWorkItem({
                businessObjectType: "customer_refund",
            }),
        ).toBe(true)
        expect(
            isCustomerRefundWorkItem({
                businessObjectType: "CustomerReceipt",
            }),
        ).toBe(false)
        expect(isCustomerRefundWorkItem(undefined)).toBe(false)
    })
})

describe("buildCustomerRefundSubmitRequest", () => {
    it("only emits version and idempotency key", () => {
        expect(
            buildCustomerRefundSubmitRequest({
                expectedVersion: 3,
                idempotencyKey: "k-crf-1",
            }),
        ).toEqual({
            expected_version: 3,
            idempotency_key: "k-crf-1",
        })
    })
})

describe("slotForCustomerRefundIntent", () => {
    it("reuses the key for the same source and reason", () => {
        const first = slotForCustomerRefundIntent(null, "src-a", "退差额")
        const retry = slotForCustomerRefundIntent(first, "src-a", "退差额")
        expect(retry.key).toBe(first.key)
        expect(retry.fingerprint).toBe(
            customerRefundIntentFingerprint("src-a", "退差额"),
        )
    })

    it("rotates when the source receipt or reason changes", () => {
        const first = slotForCustomerRefundIntent(null, "src-a", "退差额")
        const otherSource = slotForCustomerRefundIntent(
            first,
            "src-b",
            "退差额",
        )
        const otherReason = slotForCustomerRefundIntent(
            otherSource,
            "src-b",
            "全额退",
        )
        expect(otherSource.key).not.toBe(first.key)
        expect(otherReason.key).not.toBe(otherSource.key)
        expect(otherSource.key.startsWith("w11-rev-src-b-")).toBe(true)
    })
})

describe("customer accounts page refund proof", () => {
    it("declares PROCESS_REQUIRED and keeps invoice on the no-approval path", () => {
        expect(CUSTOMER_ACCOUNTS_REFUND_APPROVAL_REQUIREMENT).toBe(
            "PROCESS_REQUIRED",
        )
        expect(CUSTOMER_ACCOUNTS_REFUND_FORBIDDEN_ACTIONS).toEqual(
            expect.arrayContaining(["选择流程", "开始处理", "退回团队"]),
        )
        const pagePath = join(
            dirname(fileURLToPath(import.meta.url)),
            "../../../app/(workspace)/finance/customer-accounts/page.tsx",
        )
        const pageSource = readFileSync(pagePath, "utf8")
        expect(pageSource).toContain("CustomerRefund 为 PROCESS_REQUIRED")
        expect(pageSource).toContain("Invoice 为 NO_APPROVAL")
        for (const label of CUSTOMER_ACCOUNTS_REFUND_FORBIDDEN_ACTIONS) {
            expect(pageSource).not.toContain(label)
        }
    })
})
