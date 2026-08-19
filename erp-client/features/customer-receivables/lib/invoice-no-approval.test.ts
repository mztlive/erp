import { readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

import { describe, expect, it } from "vitest"

import {
    CUSTOMER_RECEIPT_DOCUMENT_TYPE,
    isCustomerReceiptWorkItem,
} from "./customer-receipt-approval"
import {
    INVOICE_APPROVAL_REQUIREMENT,
    INVOICE_DOCUMENT_TYPE,
    INVOICE_DTO_HAS_NO_APPROVAL,
    INVOICE_OBJECT_TYPE,
    INVOICE_ROW_HAS_NO_APPROVAL,
    invoiceActionsExcludeApproval,
    isInvoiceWorkItem,
    stripInvoiceApprovalField,
} from "./invoice-no-approval"
import {
    CUSTOMER_ACCOUNTS_INVOICE_APPROVAL_REQUIREMENT,
    CUSTOMER_ACCOUNTS_INVOICE_FORBIDDEN_ACTIONS,
} from "@/app/(workspace)/finance/customer-accounts/invoice-page-proof"

const here = dirname(fileURLToPath(import.meta.url))
const featureRoot = join(here, "..")
const pagePath = join(
    here,
    "../../../app/(workspace)/finance/customer-accounts/page.tsx",
)

const APPROVAL_ZONE_TOKENS = [
    "DefinitionBindingCard",
    "DecisionDialog",
    "ReassignDialog",
    "CancelApprovalDialog",
    "SubmissionRouteConfirmation",
    "RuntimeSummary",
    "ExecutionHistory",
    "ApprovalActionBar",
    "CustomerReceiptApprovalArea",
    "CustomerReceiptSubmitConfirmDialog",
    "CustomerRefundApprovalArea",
    "CustomerRefundSubmitConfirmDialog",
    "CustomerRefundRequestDialog",
] as const

function sourceMentionsApprovalZone(source: string): boolean {
    return APPROVAL_ZONE_TOKENS.some((token) => source.includes(token))
}

function readFeature(relativePath: string): string {
    return readFileSync(join(featureRoot, relativePath), "utf8")
}

describe("INVOICE_DOCUMENT_TYPE", () => {
    it("uses the contract type and does not alias receipts or refunds", () => {
        expect(INVOICE_DOCUMENT_TYPE).toBe("Invoice")
        expect(INVOICE_OBJECT_TYPE).toBe("invoice")
        expect(INVOICE_APPROVAL_REQUIREMENT).toBe("NO_APPROVAL")
        expect(INVOICE_DOCUMENT_TYPE).not.toBe(CUSTOMER_RECEIPT_DOCUMENT_TYPE)
        expect(INVOICE_DOCUMENT_TYPE).not.toBe("CustomerRefund")
        expect(INVOICE_DTO_HAS_NO_APPROVAL).toBe(true)
        expect(INVOICE_ROW_HAS_NO_APPROVAL).toBe(true)
    })
})

describe("isInvoiceWorkItem", () => {
    it("accepts only the contract and object-type literals", () => {
        expect(isInvoiceWorkItem({ businessObjectType: "Invoice" })).toBe(true)
        expect(isInvoiceWorkItem({ businessObjectType: "invoice" })).toBe(true)
        expect(
            isInvoiceWorkItem({ businessObjectType: "CustomerReceipt" }),
        ).toBe(false)
        expect(isInvoiceWorkItem(undefined)).toBe(false)
        expect(
            isCustomerReceiptWorkItem({ businessObjectType: "Invoice" }),
        ).toBe(false)
    })
})

describe("stripInvoiceApprovalField", () => {
    it("drops a stray approval field and leaves invoice facts intact", () => {
        const stripped = stripInvoiceApprovalField({
            id: "inv-1",
            invoice_no: "FP-1",
            approval: {
                requirement: "PROCESS_REQUIRED",
                allowed_actions: ["SUBMIT"],
            },
        })
        expect(stripped).toEqual({ id: "inv-1", invoice_no: "FP-1" })
        expect("approval" in stripped).toBe(false)
        expect(stripInvoiceApprovalField({ id: "inv-2" })).toEqual({
            id: "inv-2",
        })
    })
})

describe("invoiceActionsExcludeApproval", () => {
    it("accepts invoice business actions and rejects approval entries", () => {
        expect(
            invoiceActionsExcludeApproval([
                "VIEW_DETAIL",
                "CONTINUE_ALLOCATE",
                "ISSUE_RED_INVOICE",
            ]),
        ).toBe(true)
        expect(invoiceActionsExcludeApproval(["APPROVE", "REJECT"])).toBe(false)
        expect(invoiceActionsExcludeApproval(["CANCEL"])).toBe(false)
        expect(invoiceActionsExcludeApproval(["UPGRADE_BINDING"])).toBe(false)
        expect(invoiceActionsExcludeApproval(["SUBMIT"])).toBe(false)
    })
})

describe("invoice source paths omit the approval zone", () => {
    it("keeps invoice-only files free of approval components", () => {
        expect(sourceMentionsApprovalZone(readFeature("components/invoice-columns.tsx"))).toBe(
            false,
        )
        expect(
            sourceMentionsApprovalZone(
                readFeature("components/session-fact-fields.tsx"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsApprovalZone(
                readFeature("pages/components/customer-receivables-header.tsx"),
            ),
        ).toBe(false)
        expect(sourceMentionsApprovalZone(readFeature("lib/invoice-no-approval.ts"))).toBe(
            false,
        )
    })

    it("does not embed the approval zone inside InvoiceDetailBody", () => {
        const source = readFeature("components/detail-bodies.tsx")
        const start = source.indexOf("export function InvoiceDetailBody")
        const end = source.indexOf("function Fact(")
        expect(start).toBeGreaterThan(-1)
        expect(end).toBeGreaterThan(start)
        expect(sourceMentionsApprovalZone(source.slice(start, end))).toBe(false)
    })

    it("routes invoice register confirmation away from the receipt approval dialog", () => {
        const source = readFeature("components/allocation-session-panel.tsx")
        expect(source).toContain("isReceipt && receiptApproval")
        expect(source).toContain("FormalActionConfirmDialog")
        expect(source).toContain("确认登记销项发票并分配")
        const invoiceConfirm = source.slice(
            source.indexOf("确认登记销项发票并分配"),
        )
        expect(sourceMentionsApprovalZone(invoiceConfirm)).toBe(false)
        expect(invoiceConfirm).not.toContain("CustomerReceiptSubmitConfirmDialog")
    })
})

describe("customer accounts page invoice proof", () => {
    it("declares NO_APPROVAL and does not wire invoice approval actions", () => {
        expect(CUSTOMER_ACCOUNTS_INVOICE_APPROVAL_REQUIREMENT).toBe(
            "NO_APPROVAL",
        )
        expect(CUSTOMER_ACCOUNTS_INVOICE_FORBIDDEN_ACTIONS).toEqual(
            expect.arrayContaining([
                "选择流程",
                "通过",
                "撤回审批",
                "改派当前审批人",
            ]),
        )
        const pageSource = readFileSync(pagePath, "utf8")
        expect(pageSource).toContain("CustomerReceivablesPage")
        expect(pageSource).toContain("Invoice 为 NO_APPROVAL")
        expect(sourceMentionsApprovalZone(pageSource)).toBe(false)
        for (const label of CUSTOMER_ACCOUNTS_INVOICE_FORBIDDEN_ACTIONS) {
            expect(pageSource).not.toContain(label)
        }
    })
})
