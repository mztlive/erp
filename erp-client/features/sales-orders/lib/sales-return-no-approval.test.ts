import { readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

import { describe, expect, it } from "vitest"

import { mapSalesReturnCase } from "@/features/sales-orders/api/sales-return-cases"
import type { BackendSalesReturnCase } from "@/features/sales-orders/api/sales-return-cases"
import {
    SALES_ORDERS_SALES_RETURN_APPROVAL_REQUIREMENT,
    SALES_ORDERS_SALES_RETURN_FORBIDDEN_ACTIONS,
} from "@/app/(workspace)/sales/orders/sales-return-page-proof"
import {
    SALES_RETURN_CASE_APPROVAL_REQUIREMENT,
    SALES_RETURN_CASE_DOCUMENT_TYPE,
    SALES_RETURN_CASE_DTO_HAS_NO_APPROVAL,
    SALES_RETURN_CASE_OBJECT_TYPE,
    SALES_RETURN_CASE_ROW_HAS_NO_APPROVAL,
    isSalesReturnCaseApprovalReviewStatus,
    isSalesReturnCaseFulfillmentDivisionStatus,
    isSalesReturnCaseWorkItem,
    salesReturnCaseActionsExcludeApproval,
    salesReturnCaseStatusLabel,
    salesReturnCaseStatusLabelIsApprovalReview,
    salesReturnCaseTypeLabel,
    salesReturnRouteLabel,
    stripSalesReturnCaseApprovalField,
} from "./sales-return-no-approval"
import { SALES_CHANGE_ORDER_DOCUMENT_TYPE } from "./sales-change-order-approval"
import { SALES_ORDER_DOCUMENT_TYPE } from "./sales-order-approval"

const here = dirname(fileURLToPath(import.meta.url))
const featureRoot = join(here, "..")
const listPagePath = join(
    here,
    "../../../app/(workspace)/sales/orders/page.tsx",
)
const detailPagePath = join(
    here,
    "../../../app/(workspace)/sales/orders/[salesOrderId]/page.tsx",
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
    "SalesOrderApprovalArea",
    "VoucherSalesOrderApprovalArea",
    "SalesChangeOrderApprovalArea",
    "SalesChangeOrderApprovalSection",
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

function seed(
    extras: Partial<BackendSalesReturnCase> = {},
): BackendSalesReturnCase {
    return {
        id: "src-1",
        return_no: "XT-2026-001",
        sales_order_id: "so-1",
        case_type: "return",
        reason: "客户拒收",
        discovered_at: 1_700_000_000,
        return_route: "company_warehouse",
        status: "draft",
        version: 1,
        created_at: 1_700_000_000,
        lines: [],
        ...extras,
    }
}

describe("SALES_RETURN_CASE_DOCUMENT_TYPE", () => {
    it("uses the contract type and does not alias sales or change orders", () => {
        expect(SALES_RETURN_CASE_DOCUMENT_TYPE).toBe("SalesReturnCase")
        expect(SALES_RETURN_CASE_OBJECT_TYPE).toBe("sales_return_case")
        expect(SALES_RETURN_CASE_APPROVAL_REQUIREMENT).toBe("NO_APPROVAL")
        expect(SALES_RETURN_CASE_DOCUMENT_TYPE).not.toBe(
            SALES_ORDER_DOCUMENT_TYPE,
        )
        expect(SALES_RETURN_CASE_DOCUMENT_TYPE).not.toBe(
            SALES_CHANGE_ORDER_DOCUMENT_TYPE,
        )
        expect(SALES_RETURN_CASE_DOCUMENT_TYPE).not.toBe("PurchaseReturnOrder")
        expect(SALES_RETURN_CASE_DTO_HAS_NO_APPROVAL).toBe(true)
        expect(SALES_RETURN_CASE_ROW_HAS_NO_APPROVAL).toBe(true)
    })
})

describe("isSalesReturnCaseWorkItem", () => {
    it("accepts only the contract and object-type literals", () => {
        expect(
            isSalesReturnCaseWorkItem({
                businessObjectType: "SalesReturnCase",
            }),
        ).toBe(true)
        expect(
            isSalesReturnCaseWorkItem({
                businessObjectType: "sales_return_case",
            }),
        ).toBe(true)
        expect(
            isSalesReturnCaseWorkItem({ businessObjectType: "SalesOrder" }),
        ).toBe(false)
        expect(
            isSalesReturnCaseWorkItem({
                businessObjectType: "SalesChangeOrder",
            }),
        ).toBe(false)
        expect(isSalesReturnCaseWorkItem(undefined)).toBe(false)
    })
})

describe("sales return fulfillment division statuses", () => {
    it("treats warehouse / procurement / finance pending as fulfillment, not approval review", () => {
        for (const status of [
            "PENDING_WAREHOUSE_ACCEPTANCE",
            "pending_warehouse_acceptance",
            "PENDING_PROCUREMENT",
            "pending_procurement",
            "PENDING_FINANCE",
            "pending_finance",
        ]) {
            expect(isSalesReturnCaseFulfillmentDivisionStatus(status)).toBe(
                true,
            )
            expect(isSalesReturnCaseApprovalReviewStatus(status)).toBe(false)
            const label = salesReturnCaseStatusLabel(status)
            expect(salesReturnCaseStatusLabelIsApprovalReview(label)).toBe(
                false,
            )
            expect(label).not.toBe("审批复核")
            expect(label).not.toBe("待审批")
            expect(label).not.toBe("审批中")
            expect(label).not.toBe("待财务复核")
        }
        expect(salesReturnCaseStatusLabel("PENDING_WAREHOUSE_ACCEPTANCE")).toBe(
            "待仓储验收",
        )
        expect(salesReturnCaseStatusLabel("PENDING_PROCUREMENT")).toBe(
            "待采购处理",
        )
        expect(salesReturnCaseStatusLabel("PENDING_FINANCE")).toBe("待财务处理")
        expect(salesReturnCaseTypeLabel("shortage")).toBe("短少")
        expect(salesReturnRouteLabel("direct_to_supplier")).toBe("直退供应商")
    })
})

describe("stripSalesReturnCaseApprovalField", () => {
    it("drops a stray approval field and leaves return facts intact", () => {
        const stripped = stripSalesReturnCaseApprovalField({
            id: "src-1",
            return_no: "XT-1",
            approval: {
                requirement: "PROCESS_REQUIRED",
                allowed_actions: ["SUBMIT"],
            },
        })
        expect(stripped).toEqual({ id: "src-1", return_no: "XT-1" })
        expect("approval" in stripped).toBe(false)
        expect(stripSalesReturnCaseApprovalField({ id: "src-2" })).toEqual({
            id: "src-2",
        })
    })
})

describe("salesReturnCaseActionsExcludeApproval", () => {
    it("accepts return business actions and rejects approval entries", () => {
        expect(salesReturnCaseActionsExcludeApproval(["VIEW_DETAIL"])).toBe(
            true,
        )
        expect(
            salesReturnCaseActionsExcludeApproval(["APPROVE", "REJECT"]),
        ).toBe(false)
        expect(salesReturnCaseActionsExcludeApproval(["CANCEL"])).toBe(false)
        expect(salesReturnCaseActionsExcludeApproval(["UPGRADE_BINDING"])).toBe(
            false,
        )
        expect(salesReturnCaseActionsExcludeApproval(["SUBMIT"])).toBe(false)
    })
})

describe("mapSalesReturnCase", () => {
    it("maps a draft without an approval projection", () => {
        const row = mapSalesReturnCase(seed())
        expect(row.returnNo).toBe("XT-2026-001")
        expect(row.statusLabel).toBe("草稿")
        expect("approval" in row).toBe(false)
        expect(salesReturnCaseActionsExcludeApproval(row.allowedActions)).toBe(
            true,
        )
    })

    it("strips a stray approval field instead of rendering a binding", () => {
        const row = mapSalesReturnCase({
            ...seed({ status: "pending_finance" }),
            approval: {
                requirement: "PROCESS_REQUIRED",
                allowed_actions: ["SUBMIT", "APPROVE"],
            },
        } as BackendSalesReturnCase & { approval: unknown })
        expect("approval" in row).toBe(false)
        expect(row.statusLabel).toBe("待财务处理")
        expect(
            salesReturnCaseStatusLabelIsApprovalReview(row.statusLabel),
        ).toBe(false)
    })
})

describe("sales return source paths omit the approval zone", () => {
    it("keeps sales-return-only files free of approval components", () => {
        expect(
            sourceMentionsApprovalZone(
                readFeature("lib/sales-return-no-approval.ts"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsApprovalZone(
                readFeature("api/sales-return-cases.ts"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsApprovalZone(
                readFeature("components/sales-return-case-facts.tsx"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsBpmInternals(
                readFeature("lib/sales-return-no-approval.ts"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsBpmInternals(
                readFeature("components/sales-return-case-facts.tsx"),
            ),
        ).toBe(false)
    })

    it("does not dispatch SalesReturnCase to sales or change-order approval areas", () => {
        const detailSource = readFeature("pages/sales-order-detail-page.tsx")
        expect(detailSource).toContain("SalesReturnCase 为 NO_APPROVAL")
        expect(detailSource).not.toContain("SalesReturnCaseApprovalArea")
        expect(detailSource).not.toContain("mapSalesReturnCaseApproval")
        const factsSource = readFeature(
            "components/sales-return-case-facts.tsx",
        )
        expect(factsSource).toContain("SalesReturnCase 为 NO_APPROVAL")
        expect(sourceMentionsApprovalZone(factsSource)).toBe(false)
    })
})

describe("sales orders page sales-return proof", () => {
    it("declares NO_APPROVAL and does not wire sales-return approval actions", () => {
        expect(SALES_ORDERS_SALES_RETURN_APPROVAL_REQUIREMENT).toBe(
            "NO_APPROVAL",
        )
        expect(SALES_ORDERS_SALES_RETURN_FORBIDDEN_ACTIONS).toEqual(
            expect.arrayContaining([
                "选择流程",
                "通过",
                "撤回审批",
                "改派当前审批人",
            ]),
        )
        const listSource = readFileSync(listPagePath, "utf8")
        const detailSource = readFileSync(detailPagePath, "utf8")
        expect(listSource).toContain("SalesOrdersListPage")
        expect(listSource).toContain("SalesReturnCase 为 NO_APPROVAL")
        expect(detailSource).toContain("SalesOrderDetailPage")
        expect(detailSource).toContain("SalesReturnCase 为 NO_APPROVAL")
        expect(sourceMentionsApprovalZone(listSource)).toBe(false)
        expect(sourceMentionsApprovalZone(detailSource)).toBe(false)
        expect(sourceMentionsBpmInternals(listSource)).toBe(false)
        expect(sourceMentionsBpmInternals(detailSource)).toBe(false)
        for (const label of SALES_ORDERS_SALES_RETURN_FORBIDDEN_ACTIONS) {
            expect(listSource).not.toContain(label)
            expect(detailSource).not.toContain(label)
        }
    })
})
