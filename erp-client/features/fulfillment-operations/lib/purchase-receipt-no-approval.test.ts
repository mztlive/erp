import { readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

import { describe, expect, it } from "vitest"

import { receiptToOperation } from "@/features/fulfillment-operations/api/documents"
import { formalFromReceipt } from "@/features/fulfillment-operations/api/outcomes"
import type { BackendPurchaseReceipt } from "@/features/fulfillment-operations/api/documents"
import {
    FULFILLMENT_PURCHASE_RECEIPT_APPROVAL_REQUIREMENT,
    FULFILLMENT_PURCHASE_RECEIPT_FORBIDDEN_ACTIONS,
} from "@/app/(workspace)/fulfillment/purchase-receipt-page-proof"
import {
    PURCHASE_RECEIPT_APPROVAL_REQUIREMENT,
    PURCHASE_RECEIPT_DOCUMENT_TYPE,
    PURCHASE_RECEIPT_DTO_HAS_NO_APPROVAL,
    PURCHASE_RECEIPT_OBJECT_TYPE,
    PURCHASE_RECEIPT_OPERATION_HAS_NO_APPROVAL,
    PURCHASE_RECEIPT_OUTCOME_HAS_NO_APPROVAL,
    isPurchaseReceiptOperation,
    isPurchaseReceiptWorkItem,
    purchaseReceiptActionsExcludeApproval,
    stripPurchaseReceiptApprovalField,
} from "./purchase-receipt-no-approval"

const here = dirname(fileURLToPath(import.meta.url))
const featureRoot = join(here, "..")
const pagePath = join(here, "../../../app/(workspace)/fulfillment/page.tsx")

const APPROVAL_ZONE_TOKENS = [
    "DefinitionBindingCard",
    "DecisionDialog",
    "ReassignDialog",
    "CancelApprovalDialog",
    "SubmissionRouteConfirmation",
    "RuntimeSummary",
    "ExecutionHistory",
    "ApprovalActionBar",
    "UpgradeBindingDialog",
    "ResumeApproverDialog",
] as const

const BPM_INTERNAL_TOKENS = [
    "ProcessKind",
    "SubjectRef",
    "TransitionPlan",
] as const

function sourceMentionsApprovalZone(source: string): boolean {
    return APPROVAL_ZONE_TOKENS.some((token) => source.includes(token))
}

function sourceMentionsBpmInternals(source: string): boolean {
    return BPM_INTERNAL_TOKENS.some((token) => source.includes(token))
}

function readFeature(relativePath: string): string {
    return readFileSync(join(featureRoot, relativePath), "utf8")
}

function receiptSeed(): BackendPurchaseReceipt {
    return {
        id: "pr-1",
        receipt_no: "RK-2026-001",
        purchase_order_id: "po-1",
        warehouse_id: "wh-1",
        status: "DRAFT",
        version: 1,
        created_at: 1_700_000_000,
    }
}

describe("PURCHASE_RECEIPT_DOCUMENT_TYPE", () => {
    it("uses the contract type and does not alias deliveries or acceptances", () => {
        expect(PURCHASE_RECEIPT_DOCUMENT_TYPE).toBe("PurchaseReceipt")
        expect(PURCHASE_RECEIPT_OBJECT_TYPE).toBe("purchase_receipt")
        expect(PURCHASE_RECEIPT_APPROVAL_REQUIREMENT).toBe("NO_APPROVAL")
        expect(PURCHASE_RECEIPT_DOCUMENT_TYPE).not.toBe("Delivery")
        expect(PURCHASE_RECEIPT_DOCUMENT_TYPE).not.toBe("CustomerAcceptance")
        expect(PURCHASE_RECEIPT_DTO_HAS_NO_APPROVAL).toBe(true)
        expect(PURCHASE_RECEIPT_OPERATION_HAS_NO_APPROVAL).toBe(true)
        expect(PURCHASE_RECEIPT_OUTCOME_HAS_NO_APPROVAL).toBe(true)
    })
})

describe("isPurchaseReceiptWorkItem", () => {
    it("accepts only the contract and object-type literals", () => {
        expect(
            isPurchaseReceiptWorkItem({
                businessObjectType: "PurchaseReceipt",
            }),
        ).toBe(true)
        expect(
            isPurchaseReceiptWorkItem({
                businessObjectType: "purchase_receipt",
            }),
        ).toBe(true)
        expect(
            isPurchaseReceiptWorkItem({ businessObjectType: "Delivery" }),
        ).toBe(false)
        expect(isPurchaseReceiptWorkItem(undefined)).toBe(false)
    })
})

describe("isPurchaseReceiptOperation", () => {
    it("accepts only the RECEIPT operation type", () => {
        expect(isPurchaseReceiptOperation({ operationType: "RECEIPT" })).toBe(
            true,
        )
        expect(
            isPurchaseReceiptOperation({ operationType: "WAREHOUSE_SHIP" }),
        ).toBe(false)
        expect(isPurchaseReceiptOperation(undefined)).toBe(false)
    })
})

describe("stripPurchaseReceiptApprovalField", () => {
    it("drops a stray approval field and leaves receipt facts intact", () => {
        const stripped = stripPurchaseReceiptApprovalField({
            id: "pr-1",
            receipt_no: "RK-1",
            approval: {
                requirement: "PROCESS_REQUIRED",
                allowed_actions: ["SUBMIT"],
            },
        })
        expect(stripped).toEqual({ id: "pr-1", receipt_no: "RK-1" })
        expect("approval" in stripped).toBe(false)
        expect(stripPurchaseReceiptApprovalField({ id: "pr-2" })).toEqual({
            id: "pr-2",
        })
    })
})

describe("purchaseReceiptActionsExcludeApproval", () => {
    it("accepts receipt business actions and rejects approval entries", () => {
        expect(
            purchaseReceiptActionsExcludeApproval([
                "VIEW_DETAIL",
                "SAVE",
                "POST",
            ]),
        ).toBe(true)
        expect(
            purchaseReceiptActionsExcludeApproval(["APPROVE", "REJECT"]),
        ).toBe(false)
        expect(purchaseReceiptActionsExcludeApproval(["CANCEL"])).toBe(false)
        expect(purchaseReceiptActionsExcludeApproval(["UPGRADE_BINDING"])).toBe(
            false,
        )
        expect(purchaseReceiptActionsExcludeApproval(["SUBMIT"])).toBe(false)
    })
})

describe("receiptToOperation", () => {
    it("maps a purchase receipt without an approval projection", () => {
        const operation = receiptToOperation(receiptSeed())
        expect(operation.operationType).toBe("RECEIPT")
        expect(operation.summary).toBe("RK-2026-001")
        expect("approval" in operation).toBe(false)
        expect(
            purchaseReceiptActionsExcludeApproval(
                operation.actionBlockers.map((blocker) => blocker.action),
            ),
        ).toBe(true)
    })

    it("strips a stray approval field instead of rendering a binding", () => {
        const operation = receiptToOperation({
            ...receiptSeed(),
            approval: {
                requirement: "PROCESS_REQUIRED",
                allowed_actions: ["SUBMIT", "APPROVE"],
            },
        } as BackendPurchaseReceipt & { approval: unknown })
        expect("approval" in operation).toBe(false)
        expect(operation.operationType).toBe("RECEIPT")
    })
})

describe("formalFromReceipt", () => {
    it("projects a posted receipt without an approval zone", () => {
        const outcome = formalFromReceipt(
            {
                ...receiptSeed(),
                status: "POSTED",
                posted_at: 1_700_000_100,
                approval: {
                    requirement: "PROCESS_REQUIRED",
                    allowed_actions: ["APPROVE"],
                },
            } as BackendPurchaseReceipt & { approval: unknown },
            {
                type: "RECEIPT",
                warehouseId: "wh-1",
                warehouseLabel: "中心仓",
                occurredAt: "2026-08-14T09:00",
                lines: [],
            },
            "pr-1",
        )
        expect(outcome.factType).toBe("PURCHASE_RECEIPT")
        expect(outcome.factNo).toBe("RK-2026-001")
        expect("approval" in outcome).toBe(false)
    })
})

describe("purchase receipt source paths omit the approval zone", () => {
    it("keeps receipt-only files free of approval components", () => {
        expect(
            sourceMentionsApprovalZone(
                readFeature("components/forms/fulfillment-receipt-form.tsx"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsApprovalZone(
                readFeature("lib/purchase-receipt-no-approval.ts"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsBpmInternals(
                readFeature("lib/purchase-receipt-no-approval.ts"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsBpmInternals(
                readFeature("components/forms/fulfillment-receipt-form.tsx"),
            ),
        ).toBe(false)
    })

    it("does not embed the approval zone inside the receipt create result", () => {
        expect(
            sourceMentionsApprovalZone(
                readFeature("pages/components/fulfillment-result-panel.tsx"),
            ),
        ).toBe(false)
        expect(sourceMentionsApprovalZone(readFeature("api/outcomes.ts"))).toBe(
            false,
        )
    })

    it("routes receipt submit confirmation through the business dialog, not an approval zone", () => {
        const source = readFeature("pages/fulfillment-operations-page.tsx")
        expect(source).toContain("FormalActionConfirmDialog")
        expect(source).toContain("OPERATION_CONFIRM_TITLE")
        expect(sourceMentionsApprovalZone(source)).toBe(false)
        expect(sourceMentionsBpmInternals(source)).toBe(false)
        expect(source).toContain("PurchaseReceipt 为 NO_APPROVAL")
    })

    it("does not dispatch RECEIPT drafts to an approval form", () => {
        const source = readFeature(
            "components/forms/fulfillment-draft-form.tsx",
        )
        const start = source.indexOf('case "RECEIPT"')
        const end = source.indexOf('case "WAREHOUSE_SHIP"')
        expect(start).toBeGreaterThan(-1)
        expect(end).toBeGreaterThan(start)
        const receiptBranch = source.slice(start, end)
        expect(receiptBranch).toContain("FulfillmentReceiptForm")
        expect(sourceMentionsApprovalZone(receiptBranch)).toBe(false)
    })
})

describe("fulfillment page purchase receipt proof", () => {
    it("declares NO_APPROVAL and does not wire receipt approval actions", () => {
        expect(FULFILLMENT_PURCHASE_RECEIPT_APPROVAL_REQUIREMENT).toBe(
            "NO_APPROVAL",
        )
        expect(FULFILLMENT_PURCHASE_RECEIPT_FORBIDDEN_ACTIONS).toEqual(
            expect.arrayContaining([
                "选择流程",
                "通过",
                "撤回审批",
                "改派当前审批人",
            ]),
        )
        const pageSource = readFileSync(pagePath, "utf8")
        expect(pageSource).toContain("FulfillmentOperationsPage")
        expect(pageSource).toContain("PurchaseReceipt 为 NO_APPROVAL")
        expect(sourceMentionsApprovalZone(pageSource)).toBe(false)
        expect(sourceMentionsBpmInternals(pageSource)).toBe(false)
        for (const label of FULFILLMENT_PURCHASE_RECEIPT_FORBIDDEN_ACTIONS) {
            expect(pageSource).not.toContain(label)
        }
    })
})
