import { readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

import { describe, expect, it } from "vitest"

import {
    buildPaymentReversalSubmitRequest,
    PAYMENT_REVERSAL_APPROVAL_REQUIREMENT,
    PAYMENT_REVERSAL_DOCUMENT_TYPE,
    PAYMENT_REVERSAL_OBJECT_TYPE,
    isPaymentReversalWorkItem,
    mapPaymentReversalApproval,
    mergePaymentReversalAllowedActions,
    paymentReversalApprovalPhase,
    paymentReversalIntentFingerprint,
    paymentReversalStatusLabel,
    paymentReversalStatusTone,
    readPaymentReversalApprovalResponsibility,
    slotForPaymentReversalIntent,
} from "./payment-reversal-approval"
import { SUPPLIER_PAYMENT_DOCUMENT_TYPE } from "./supplier-payment-approval"
import { SUPPLIER_REFUND_DOCUMENT_TYPE } from "./supplier-refund-approval"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import {
    approvalConflictMessage,
    isApprovalConflict,
} from "@/features/approval-workflow/api"
import {
    SUPPLIER_ACCOUNTS_PAYMENT_REVERSAL_APPROVAL_REQUIREMENT,
    SUPPLIER_ACCOUNTS_PAYMENT_REVERSAL_FORBIDDEN_ACTIONS,
} from "@/app/(workspace)/finance/supplier-accounts/payment-reversal-page-proof"

const BPM_INTERNAL_TOKENS = [
    "ProcessKind",
    "SubjectRef",
    "TransitionPlan",
] as const

const binding: DocumentApprovalView = {
    requirement: "PROCESS_REQUIRED",
    definition: {
        id: "def-pr-1",
        name: "付款冲正审批",
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

const featureRoot = join(dirname(fileURLToPath(import.meta.url)), "..")

function readFeature(relativePath: string): string {
    return readFileSync(join(featureRoot, relativePath), "utf8")
}

describe("PAYMENT_REVERSAL_DOCUMENT_TYPE", () => {
    it("uses the contract document type and does not alias payment or refund", () => {
        expect(PAYMENT_REVERSAL_DOCUMENT_TYPE).toBe("PaymentReversal")
        expect(PAYMENT_REVERSAL_OBJECT_TYPE).toBe("payment_reversal")
        expect(PAYMENT_REVERSAL_APPROVAL_REQUIREMENT).toBe("PROCESS_REQUIRED")
        expect(PAYMENT_REVERSAL_DOCUMENT_TYPE).not.toBe(
            SUPPLIER_PAYMENT_DOCUMENT_TYPE,
        )
        expect(PAYMENT_REVERSAL_DOCUMENT_TYPE).not.toBe(
            SUPPLIER_REFUND_DOCUMENT_TYPE,
        )
    })
})

describe("paymentReversalStatusLabel", () => {
    it("maps server codes to Chinese and never prints enum leftovers", () => {
        expect(paymentReversalStatusLabel("DRAFT")).toBe("草稿")
        expect(paymentReversalStatusLabel("IN_APPROVAL")).toBe("审批中")
        expect(paymentReversalStatusLabel("pending_review")).toBe("审批中")
        expect(paymentReversalStatusLabel("POSTED")).toBe("已过账")
        expect(paymentReversalStatusLabel("REVERSED")).toBe("已冲正")
        expect(paymentReversalStatusLabel("UNKNOWN")).toBe("冲正单")
        expect(paymentReversalStatusTone("IN_APPROVAL")).toBe("warning")
        expect(paymentReversalStatusTone("POSTED")).toBe("success")
    })
})

describe("paymentReversalApprovalPhase", () => {
    it("keeps unsubmitted documents on the binding card", () => {
        expect(paymentReversalApprovalPhase(binding, "DRAFT")).toBe("draft")
        expect(paymentReversalApprovalPhase(undefined, undefined)).toBe("draft")
    })

    it("switches to runtime when an instance exists", () => {
        expect(
            paymentReversalApprovalPhase(
                {
                    ...binding,
                    instance: {
                        id: "inst-pr-1",
                        status: "RUNNING",
                        currentRoundNo: 1,
                    },
                },
                "DRAFT",
            ),
        ).toBe("runtime")
        expect(paymentReversalApprovalPhase(binding, "IN_APPROVAL")).toBe(
            "runtime",
        )
    })
})

describe("mapPaymentReversalApproval", () => {
    it("maps the created binding without turning it into a work item", () => {
        const view = mapPaymentReversalApproval({
            requirement: "PROCESS_REQUIRED",
            definition: {
                id: "def-pr-1",
                name: "付款冲正审批",
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
        expect(view?.definition?.name).toBe("付款冲正审批")
        expect(view?.definition?.nodes[1]?.assigneeName).toBe("李四")
        expect(view?.allowedActions).toEqual(["SUBMIT", "UPGRADE_BINDING"])
    })

    it("returns undefined when the document has no approval projection", () => {
        expect(mapPaymentReversalApproval(null)).toBeUndefined()
        expect(mapPaymentReversalApproval(undefined)).toBeUndefined()
    })
})

describe("mergePaymentReversalAllowedActions", () => {
    it("unions server facts and drops generic WorkItem actions", () => {
        expect(
            mergePaymentReversalAllowedActions(
                ["CANCEL"],
                ["APPROVE", "REASSIGN", "CLOSE"],
            ),
        ).toEqual(["CANCEL", "APPROVE"])
    })
})

describe("readPaymentReversalApprovalResponsibility", () => {
    it("reads only instance fields and does not fall back to review path or the first node", () => {
        expect(
            readPaymentReversalApprovalResponsibility({
                ...binding,
                instance: {
                    id: "inst-pr-1",
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
        expect(readPaymentReversalApprovalResponsibility(binding)).toEqual({
            nextResponsible: undefined,
            currentNodeLabel: undefined,
        })
    })
})

describe("isPaymentReversalWorkItem", () => {
    it("accepts only the contract and object-type literals", () => {
        expect(
            isPaymentReversalWorkItem({
                businessObjectType: "PaymentReversal",
            }),
        ).toBe(true)
        expect(
            isPaymentReversalWorkItem({
                businessObjectType: "payment_reversal",
            }),
        ).toBe(true)
        expect(
            isPaymentReversalWorkItem({
                businessObjectType: "SupplierPayment",
            }),
        ).toBe(false)
        expect(
            isPaymentReversalWorkItem({
                businessObjectType: "SupplierRefund",
            }),
        ).toBe(false)
        expect(isPaymentReversalWorkItem(undefined)).toBe(false)
    })
})

describe("buildPaymentReversalSubmitRequest", () => {
    it("only emits version and idempotency key", () => {
        expect(
            buildPaymentReversalSubmitRequest({
                expectedVersion: 3,
                idempotencyKey: "k-pr-1",
            }),
        ).toEqual({
            expected_version: 3,
            idempotency_key: "k-pr-1",
        })
    })
})

describe("slotForPaymentReversalIntent", () => {
    it("reuses the key for the same source and reason", () => {
        const first = slotForPaymentReversalIntent(null, "src-a", "录入错误")
        const retry = slotForPaymentReversalIntent(first, "src-a", "录入错误")
        expect(retry.key).toBe(first.key)
        expect(retry.fingerprint).toBe(
            paymentReversalIntentFingerprint("src-a", "录入错误"),
        )
    })

    it("rotates when the source payment or reason changes", () => {
        const first = slotForPaymentReversalIntent(null, "src-a", "录入错误")
        const otherSource = slotForPaymentReversalIntent(
            first,
            "src-b",
            "录入错误",
        )
        const otherReason = slotForPaymentReversalIntent(
            otherSource,
            "src-b",
            "金额错误",
        )
        expect(otherSource.key).not.toBe(first.key)
        expect(otherReason.key).not.toBe(otherSource.key)
        expect(otherSource.key.startsWith("w12-pr-src-b-")).toBe(true)
    })
})

describe("payment reversal 409 does not auto-replay", () => {
    it("identifies 409 as a responsibility change and never retries the decision", () => {
        expect(isApprovalConflict({ status: 409 })).toBe(true)
        expect(approvalConflictMessage({ status: 409 })).toBe(
            "责任或版本已变化，请刷新后重新确认",
        )
        const areaSource = readFeature(
            "components/payment-reversal-approval-area.tsx",
        )
        expect(areaSource).not.toContain("mutateAsync(")
        expect(areaSource).not.toContain("retry")
        expect(areaSource).not.toContain("RETRY_CURRENT_STEP")
    })
})

describe("payment reversal pages have no BPM internals", () => {
    it("does not mention ProcessKind, SubjectRef or TransitionPlan", () => {
        const sources = [
            readFeature("lib/payment-reversal-approval.ts"),
            readFeature("components/payment-reversal-approval-area.tsx"),
            readFeature("components/payment-reversal-request-dialog.tsx"),
            readFeature(
                "components/payment-reversal-submit-confirm-dialog.tsx",
            ),
            readFeature("components/payment-reversal-detail-body.tsx"),
            readFeature("api/reversals.ts"),
            readFeature("pages/hooks/use-payment-reversal-flow.ts"),
        ]
        for (const source of sources) {
            for (const token of BPM_INTERNAL_TOKENS) {
                expect(source).not.toContain(token)
            }
        }
    })
})

describe("supplier accounts page payment reversal proof", () => {
    it("declares PROCESS_REQUIRED and forbids choosing a process", () => {
        expect(SUPPLIER_ACCOUNTS_PAYMENT_REVERSAL_APPROVAL_REQUIREMENT).toBe(
            "PROCESS_REQUIRED",
        )
        expect(SUPPLIER_ACCOUNTS_PAYMENT_REVERSAL_FORBIDDEN_ACTIONS).toEqual(
            expect.arrayContaining(["选择流程", "转交", "关闭任务"]),
        )
        const pagePath = join(
            dirname(fileURLToPath(import.meta.url)),
            "../../../app/(workspace)/finance/supplier-accounts/page.tsx",
        )
        const pageSource = readFileSync(pagePath, "utf8")
        expect(pageSource).toContain("PaymentReversal 为 PROCESS_REQUIRED")
        expect(pageSource).toContain("Invoice 为 NO_APPROVAL")
        for (const label of SUPPLIER_ACCOUNTS_PAYMENT_REVERSAL_FORBIDDEN_ACTIONS) {
            expect(pageSource).not.toContain(label)
        }
    })
})
