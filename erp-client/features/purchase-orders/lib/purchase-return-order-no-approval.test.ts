import { readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

import { describe, expect, it } from "vitest"

import {
    isPurchaseChangeOrderWorkItem,
    PURCHASE_CHANGE_ORDER_DOCUMENT_TYPE,
} from "./purchase-change-order-approval"
import {
    PURCHASE_ORDER_DOCUMENT_TYPE,
} from "./purchase-order-approval"
import {
    isPurchaseReturnExecutionStatus,
    isPurchaseReturnOrderWorkItem,
    PURCHASE_RETURN_ORDER_APPROVAL_REQUIREMENT,
    PURCHASE_RETURN_ORDER_DOCUMENT_TYPE,
    PURCHASE_RETURN_ORDER_DTO_HAS_NO_APPROVAL,
    PURCHASE_RETURN_ORDER_OBJECT_TYPE,
    PURCHASE_RETURN_ORDER_ROW_HAS_NO_APPROVAL,
    purchaseReturnActionsExcludeApproval,
    purchaseReturnModeLabel,
    purchaseReturnOrderStatusLabel,
    purchaseReturnOrderStatusTone,
    stripPurchaseReturnApprovalField,
} from "./purchase-return-order-no-approval"
import {
    PROCUREMENT_PURCHASE_RETURN_APPROVAL_REQUIREMENT,
    PROCUREMENT_PURCHASE_RETURN_FORBIDDEN_ACTIONS,
} from "@/app/(workspace)/procurement/orders/purchase-return-page-proof"

const here = dirname(fileURLToPath(import.meta.url))
const featureRoot = join(here, "..")
const listPagePath = join(
    here,
    "../../../app/(workspace)/procurement/orders/page.tsx",
)
const detailPagePath = join(
    here,
    "../../../app/(workspace)/procurement/orders/[purchaseOrderId]/page.tsx",
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
    "UpgradeBindingDialog",
    "ResumeApproverDialog",
    "PurchaseOrderApprovalArea",
    "PurchaseChangeOrderApprovalArea",
    "PurchaseChangeOrderApprovalSection",
    "PurchaseChangeOrderSubmitConfirmDialog",
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

describe("PURCHASE_RETURN_ORDER_DOCUMENT_TYPE", () => {
    it("uses the contract type and does not alias purchase orders", () => {
        expect(PURCHASE_RETURN_ORDER_DOCUMENT_TYPE).toBe("PurchaseReturnOrder")
        expect(PURCHASE_RETURN_ORDER_OBJECT_TYPE).toBe("purchase_return_order")
        expect(PURCHASE_RETURN_ORDER_APPROVAL_REQUIREMENT).toBe("NO_APPROVAL")
        expect(PURCHASE_RETURN_ORDER_DOCUMENT_TYPE).not.toBe(
            PURCHASE_ORDER_DOCUMENT_TYPE,
        )
        expect(PURCHASE_RETURN_ORDER_DOCUMENT_TYPE).not.toBe(
            PURCHASE_CHANGE_ORDER_DOCUMENT_TYPE,
        )
        expect(PURCHASE_RETURN_ORDER_DTO_HAS_NO_APPROVAL).toBe(true)
        expect(PURCHASE_RETURN_ORDER_ROW_HAS_NO_APPROVAL).toBe(true)
    })
})

describe("isPurchaseReturnOrderWorkItem", () => {
    it("accepts only the contract and object-type literals", () => {
        expect(
            isPurchaseReturnOrderWorkItem({
                businessObjectType: "PurchaseReturnOrder",
            }),
        ).toBe(true)
        expect(
            isPurchaseReturnOrderWorkItem({
                businessObjectType: "purchase_return_order",
            }),
        ).toBe(true)
        expect(
            isPurchaseReturnOrderWorkItem({
                businessObjectType: "PurchaseOrder",
            }),
        ).toBe(false)
        expect(
            isPurchaseReturnOrderWorkItem({
                businessObjectType: "PurchaseChangeOrder",
            }),
        ).toBe(false)
        expect(isPurchaseReturnOrderWorkItem(undefined)).toBe(false)
        expect(
            isPurchaseChangeOrderWorkItem({
                businessObjectType: "PurchaseReturnOrder",
            }),
        ).toBe(false)
    })
})

describe("purchaseReturnOrderStatusLabel", () => {
    it("maps PENDING_EXECUTION to 待执行 and never as approval review", () => {
        expect(purchaseReturnOrderStatusLabel("PENDING_EXECUTION")).toBe(
            "待执行",
        )
        expect(purchaseReturnOrderStatusLabel("pending_execution")).toBe(
            "待执行",
        )
        expect(purchaseReturnOrderStatusLabel("PENDING_EXECUTION")).not.toBe(
            "审批中",
        )
        expect(purchaseReturnOrderStatusLabel("PENDING_EXECUTION")).not.toBe(
            "审批复核",
        )
        expect(purchaseReturnOrderStatusLabel("pending_execution")).not.toBe(
            "审批中",
        )
        expect(isPurchaseReturnExecutionStatus("PENDING_EXECUTION")).toBe(true)
        expect(isPurchaseReturnExecutionStatus("pending_execution")).toBe(true)
        expect(isPurchaseReturnExecutionStatus("IN_APPROVAL")).toBe(false)
        expect(purchaseReturnOrderStatusTone("PENDING_EXECUTION")).toBe(
            "warning",
        )
        expect(purchaseReturnOrderStatusLabel("DRAFT")).toBe("草稿")
        expect(purchaseReturnOrderStatusLabel("draft")).toBe("草稿")
        expect(purchaseReturnOrderStatusLabel("returned")).toBe("已退货")
        expect(purchaseReturnOrderStatusLabel("completed")).toBe("已完成")
        expect(purchaseReturnOrderStatusLabel("voided")).toBe("作废")
        expect(purchaseReturnOrderStatusLabel("UNKNOWN")).toBe("采购退货")
        expect(purchaseReturnModeLabel("company_warehouse_to_supplier")).toBe(
            "公司仓退供应商",
        )
        expect(purchaseReturnModeLabel("direct_to_supplier")).toBe(
            "客户直退供应商",
        )
    })
})

describe("stripPurchaseReturnApprovalField", () => {
    it("drops a stray approval field and leaves return facts intact", () => {
        const stripped = stripPurchaseReturnApprovalField({
            id: "pro-1",
            purchase_return_no: "TH-1",
            approval: {
                requirement: "PROCESS_REQUIRED",
                allowed_actions: ["SUBMIT"],
            },
        })
        expect(stripped).toEqual({ id: "pro-1", purchase_return_no: "TH-1" })
        expect("approval" in stripped).toBe(false)
        expect(stripPurchaseReturnApprovalField({ id: "pro-2" })).toEqual({
            id: "pro-2",
        })
    })
})

describe("purchaseReturnActionsExcludeApproval", () => {
    it("accepts return business actions and rejects approval entries", () => {
        expect(
            purchaseReturnActionsExcludeApproval([
                "VIEW_DETAIL",
                "SAVE",
                "EXECUTE",
            ]),
        ).toBe(true)
        expect(purchaseReturnActionsExcludeApproval(["APPROVE", "REJECT"])).toBe(
            false,
        )
        expect(purchaseReturnActionsExcludeApproval(["CANCEL"])).toBe(false)
        expect(purchaseReturnActionsExcludeApproval(["UPGRADE_BINDING"])).toBe(
            false,
        )
        expect(purchaseReturnActionsExcludeApproval(["SUBMIT"])).toBe(false)
    })
})

describe("purchase return source paths omit the approval zone", () => {
    it("keeps return-only files free of approval components", () => {
        expect(
            sourceMentionsApprovalZone(
                readFeature("lib/purchase-return-order-no-approval.ts"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsApprovalZone(
                readFeature("components/purchase-return-order-section.tsx"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsApprovalZone(
                readFeature("api/purchase-return-orders.ts"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsBpmInternals(
                readFeature("lib/purchase-return-order-no-approval.ts"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsBpmInternals(
                readFeature("components/purchase-return-order-section.tsx"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsBpmInternals(
                readFeature("api/purchase-return-orders.ts"),
            ),
        ).toBe(false)
    })

    it("does not embed the approval zone inside the related return section", () => {
        const source = readFeature(
            "components/purchase-order-detail-changes-section.tsx",
        )
        expect(source).toContain("PurchaseReturnOrderRelatedSection")
        const returnSlice = source.slice(
            source.lastIndexOf("<PurchaseReturnOrderRelatedSection"),
        )
        expect(sourceMentionsApprovalZone(returnSlice)).toBe(false)
        expect(source).toContain("PurchaseReturnOrder 为 NO_APPROVAL")
    })
})

describe("procurement orders page purchase return proof", () => {
    it("declares NO_APPROVAL and does not wire return approval actions", () => {
        expect(PROCUREMENT_PURCHASE_RETURN_APPROVAL_REQUIREMENT).toBe(
            "NO_APPROVAL",
        )
        expect(PROCUREMENT_PURCHASE_RETURN_FORBIDDEN_ACTIONS).toEqual(
            expect.arrayContaining([
                "选择流程",
                "通过",
                "撤回审批",
                "改派当前审批人",
            ]),
        )
        const listPageSource = readFileSync(listPagePath, "utf8")
        expect(listPageSource).toContain("PurchaseOrdersListPage")
        expect(listPageSource).toContain("PurchaseReturnOrder 为 NO_APPROVAL")
        expect(sourceMentionsApprovalZone(listPageSource)).toBe(false)
        expect(sourceMentionsBpmInternals(listPageSource)).toBe(false)
        for (const label of PROCUREMENT_PURCHASE_RETURN_FORBIDDEN_ACTIONS) {
            expect(listPageSource).not.toContain(label)
        }

        const detailPageSource = readFileSync(detailPagePath, "utf8")
        expect(detailPageSource).toContain("PurchaseOrderDetailPage")
        expect(detailPageSource).toContain("PurchaseReturnOrder 为 NO_APPROVAL")
        expect(detailPageSource).toContain("PENDING_EXECUTION")
        expect(sourceMentionsApprovalZone(detailPageSource)).toBe(false)
        expect(sourceMentionsBpmInternals(detailPageSource)).toBe(false)
        for (const label of PROCUREMENT_PURCHASE_RETURN_FORBIDDEN_ACTIONS) {
            expect(detailPageSource).not.toContain(label)
        }
    })
})
