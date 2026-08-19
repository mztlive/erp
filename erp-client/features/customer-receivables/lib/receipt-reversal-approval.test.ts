import { readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

import { describe, expect, it } from "vitest"

import {
    buildReceiptReversalSubmitRequest,
    RECEIPT_REVERSAL_APPROVAL_REQUIREMENT,
    RECEIPT_REVERSAL_DOCUMENT_TYPE,
    RECEIPT_REVERSAL_OBJECT_TYPE,
    receiptReversalApprovalPhase,
    receiptReversalStatusLabel,
    receiptReversalStatusTone,
    isReceiptReversalWorkItem,
    receiptReversalIntentFingerprint,
    mapReceiptReversalApproval,
    mergeReceiptReversalAllowedActions,
    readReceiptReversalApprovalResponsibility,
    slotForReceiptReversalIntent,
} from "./receipt-reversal-approval"
import { CUSTOMER_RECEIPT_DOCUMENT_TYPE } from "./customer-receipt-approval"
import { CUSTOMER_REFUND_DOCUMENT_TYPE } from "./customer-refund-approval"
import { INVOICE_DOCUMENT_TYPE } from "./invoice-no-approval"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import {
    CUSTOMER_ACCOUNTS_RECEIPT_REVERSAL_APPROVAL_REQUIREMENT,
    CUSTOMER_ACCOUNTS_RECEIPT_REVERSAL_FORBIDDEN_ACTIONS,
} from "@/app/(workspace)/finance/customer-accounts/receipt-reversal-page-proof"

const binding: DocumentApprovalView = {
    requirement: "PROCESS_REQUIRED",
    definition: {
        id: "def-rr-1",
        name: "回款冲正审批",
        version: 2,
        nodes: [
            { key: "n1", name: "冲正复核", assigneeName: "张三" },
            { key: "n2", name: "财务确认", assigneeName: "李四" },
        ],
        publishedNodes: [],
    },
    recentHistory: [],
    historyHasMore: false,
    allowedActions: ["SUBMIT"],
}

describe("RECEIPT_REVERSAL_DOCUMENT_TYPE", () => {
    it("uses the contract document type and does not alias receipt, refund or invoice", () => {
        expect(RECEIPT_REVERSAL_DOCUMENT_TYPE).toBe("ReceiptReversal")
        expect(RECEIPT_REVERSAL_OBJECT_TYPE).toBe("receipt_reversal")
        expect(RECEIPT_REVERSAL_APPROVAL_REQUIREMENT).toBe("PROCESS_REQUIRED")
        expect(RECEIPT_REVERSAL_DOCUMENT_TYPE).not.toBe(
            CUSTOMER_RECEIPT_DOCUMENT_TYPE,
        )
        expect(RECEIPT_REVERSAL_DOCUMENT_TYPE).not.toBe(
            CUSTOMER_REFUND_DOCUMENT_TYPE,
        )
        expect(RECEIPT_REVERSAL_DOCUMENT_TYPE).not.toBe(INVOICE_DOCUMENT_TYPE)
    })
})

describe("receiptReversalStatusLabel", () => {
    it("maps server codes to Chinese and never prints enum leftovers", () => {
        expect(receiptReversalStatusLabel("DRAFT")).toBe("草稿")
        expect(receiptReversalStatusLabel("IN_APPROVAL")).toBe("审批中")
        expect(receiptReversalStatusLabel("pending_review")).toBe("审批中")
        expect(receiptReversalStatusLabel("POSTED")).toBe("已过账")
        expect(receiptReversalStatusLabel("REVERSED")).toBe("已冲正")
        expect(receiptReversalStatusLabel("UNKNOWN")).toBe("冲正单")
        expect(receiptReversalStatusTone("IN_APPROVAL")).toBe("warning")
        expect(receiptReversalStatusTone("POSTED")).toBe("success")
    })
})

describe("receiptReversalApprovalPhase", () => {
    it("keeps unsubmitted documents on the binding card", () => {
        expect(receiptReversalApprovalPhase(binding, "DRAFT")).toBe("draft")
        expect(receiptReversalApprovalPhase(undefined, undefined)).toBe("draft")
    })

    it("switches to runtime when an instance exists", () => {
        expect(
            receiptReversalApprovalPhase(
                {
                    ...binding,
                    instance: {
                        id: "inst-rr-1",
                        status: "RUNNING",
                        currentRoundNo: 1,
                    },
                },
                "DRAFT",
            ),
        ).toBe("runtime")
        expect(receiptReversalApprovalPhase(binding, "IN_APPROVAL")).toBe(
            "runtime",
        )
    })
})

describe("mapReceiptReversalApproval", () => {
    it("maps the created binding without turning it into a work item", () => {
        const view = mapReceiptReversalApproval({
            requirement: "PROCESS_REQUIRED",
            definition: {
                id: "def-rr-1",
                name: "回款冲正审批",
                version: 2,
                nodes: [
                    { key: "n1", name: "冲正复核", assignee_name: "张三" },
                    { key: "n2", name: "财务确认", assignee_name: "李四" },
                ],
            },
            instance: null,
            recent_history: [],
            allowed_actions: ["SUBMIT", "UPGRADE_BINDING"],
        })
        expect(view?.instance).toBeUndefined()
        expect(view?.definition?.name).toBe("回款冲正审批")
        expect(view?.definition?.nodes[1]?.assigneeName).toBe("李四")
        expect(view?.allowedActions).toEqual(["SUBMIT", "UPGRADE_BINDING"])
    })

    it("returns undefined when the document has no approval projection", () => {
        expect(mapReceiptReversalApproval(null)).toBeUndefined()
        expect(mapReceiptReversalApproval(undefined)).toBeUndefined()
    })
})

describe("mergeReceiptReversalAllowedActions", () => {
    it("unions server facts and drops start-processing or pool actions", () => {
        expect(
            mergeReceiptReversalAllowedActions(
                ["CANCEL"],
                ["APPROVE", "START_PROCESSING", "RELEASE_TO_TEAM"],
            ),
        ).toEqual(["CANCEL", "APPROVE"])
    })
})

describe("readReceiptReversalApprovalResponsibility", () => {
    it("reads only instance fields and does not fall back to review path or the first node", () => {
        expect(
            readReceiptReversalApprovalResponsibility({
                ...binding,
                instance: {
                    id: "inst-rr-1",
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
        expect(readReceiptReversalApprovalResponsibility(binding)).toEqual({
            nextResponsible: undefined,
            currentNodeLabel: undefined,
        })
    })
})

describe("isReceiptReversalWorkItem", () => {
    it("accepts only the contract and object-type literals", () => {
        expect(
            isReceiptReversalWorkItem({
                businessObjectType: "ReceiptReversal",
            }),
        ).toBe(true)
        expect(
            isReceiptReversalWorkItem({
                businessObjectType: "receipt_reversal",
            }),
        ).toBe(true)
        expect(
            isReceiptReversalWorkItem({
                businessObjectType: "CustomerRefund",
            }),
        ).toBe(false)
        expect(
            isReceiptReversalWorkItem({
                businessObjectType: "CustomerReceipt",
            }),
        ).toBe(false)
        expect(isReceiptReversalWorkItem(undefined)).toBe(false)
    })
})

describe("buildReceiptReversalSubmitRequest", () => {
    it("only emits version and idempotency key", () => {
        expect(
            buildReceiptReversalSubmitRequest({
                expectedVersion: 3,
                idempotencyKey: "k-rr-1",
            }),
        ).toEqual({
            expected_version: 3,
            idempotency_key: "k-rr-1",
        })
    })
})

describe("slotForReceiptReversalIntent", () => {
    it("reuses the key for the same source and reason", () => {
        const first = slotForReceiptReversalIntent(null, "src-a", "录入错误")
        const retry = slotForReceiptReversalIntent(first, "src-a", "录入错误")
        expect(retry.key).toBe(first.key)
        expect(retry.fingerprint).toBe(
            receiptReversalIntentFingerprint("src-a", "录入错误"),
        )
    })

    it("rotates when the source receipt or reason changes", () => {
        const first = slotForReceiptReversalIntent(null, "src-a", "录入错误")
        const otherSource = slotForReceiptReversalIntent(
            first,
            "src-b",
            "录入错误",
        )
        const otherReason = slotForReceiptReversalIntent(
            otherSource,
            "src-b",
            "金额错误",
        )
        expect(otherSource.key).not.toBe(first.key)
        expect(otherReason.key).not.toBe(otherSource.key)
        expect(otherSource.key.startsWith("w11-rr-src-b-")).toBe(true)
    })
})

describe("customer accounts page receipt reversal proof", () => {
    it("declares PROCESS_REQUIRED and keeps invoice on the no-approval path", () => {
        expect(CUSTOMER_ACCOUNTS_RECEIPT_REVERSAL_APPROVAL_REQUIREMENT).toBe(
            "PROCESS_REQUIRED",
        )
        expect(CUSTOMER_ACCOUNTS_RECEIPT_REVERSAL_FORBIDDEN_ACTIONS).toEqual(
            expect.arrayContaining(["选择流程", "开始处理", "退回团队"]),
        )
        const pagePath = join(
            dirname(fileURLToPath(import.meta.url)),
            "../../../app/(workspace)/finance/customer-accounts/page.tsx",
        )
        const pageSource = readFileSync(pagePath, "utf8")
        expect(pageSource).toContain("ReceiptReversal 为 PROCESS_REQUIRED")
        expect(pageSource).toContain("Invoice 为 NO_APPROVAL")
        for (const label of CUSTOMER_ACCOUNTS_RECEIPT_REVERSAL_FORBIDDEN_ACTIONS) {
            expect(pageSource).not.toContain(label)
        }
    })
})
