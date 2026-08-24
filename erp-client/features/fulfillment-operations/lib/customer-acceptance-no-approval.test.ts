import { readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

import { describe, expect, it } from "vitest"

import { formalFromDelivery } from "@/features/fulfillment-operations/api/outcomes"
import type { BackendDelivery } from "@/features/fulfillment-operations/api/documents"
import {
    FULFILLMENT_CUSTOMER_ACCEPTANCE_APPROVAL_REQUIREMENT,
    FULFILLMENT_CUSTOMER_ACCEPTANCE_FORBIDDEN_ACTIONS,
} from "@/app/(workspace)/fulfillment/customer-acceptance-page-proof"
import {
    CUSTOMER_ACCEPTANCE_APPROVAL_REQUIREMENT,
    CUSTOMER_ACCEPTANCE_DOCUMENT_TYPE,
    CUSTOMER_ACCEPTANCE_DTO_HAS_NO_APPROVAL,
    CUSTOMER_ACCEPTANCE_OBJECT_TYPE,
    CUSTOMER_ACCEPTANCE_OPERATION_HAS_NO_APPROVAL,
    CUSTOMER_ACCEPTANCE_OUTCOME_HAS_NO_APPROVAL,
    customerAcceptanceActionsExcludeApproval,
    isCustomerAcceptanceHandoff,
    isCustomerAcceptanceWorkItem,
    stripCustomerAcceptanceApprovalField,
} from "./customer-acceptance-no-approval"

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

function deliverySeed(): BackendDelivery {
    return {
        id: "dl-1",
        delivery_no: "FH-2026-001",
        delivery_type: "WAREHOUSE_SHIP",
        sales_order_id: "so-1",
        warehouse_id: "wh-1",
        status: "SHIPPED",
        version: 2,
        created_at: 1_700_000_000,
        shipped_at: 1_700_000_100,
    }
}

describe("CUSTOMER_ACCEPTANCE_DOCUMENT_TYPE", () => {
    it("uses the contract type and does not alias receipts or fulfillments", () => {
        expect(CUSTOMER_ACCEPTANCE_DOCUMENT_TYPE).toBe("CustomerAcceptance")
        expect(CUSTOMER_ACCEPTANCE_OBJECT_TYPE).toBe("customer_acceptance")
        expect(CUSTOMER_ACCEPTANCE_APPROVAL_REQUIREMENT).toBe("NO_APPROVAL")
        expect(CUSTOMER_ACCEPTANCE_DOCUMENT_TYPE).not.toBe("PurchaseReceipt")
        expect(CUSTOMER_ACCEPTANCE_DOCUMENT_TYPE).not.toBe("Delivery")
        expect(CUSTOMER_ACCEPTANCE_DOCUMENT_TYPE).not.toBe("ElectronicDelivery")
        expect(CUSTOMER_ACCEPTANCE_DOCUMENT_TYPE).not.toBe("ServiceFulfillment")
        expect(CUSTOMER_ACCEPTANCE_DTO_HAS_NO_APPROVAL).toBe(true)
        expect(CUSTOMER_ACCEPTANCE_OPERATION_HAS_NO_APPROVAL).toBe(true)
        expect(CUSTOMER_ACCEPTANCE_OUTCOME_HAS_NO_APPROVAL).toBe(true)
    })
})

describe("isCustomerAcceptanceWorkItem", () => {
    it("accepts only the contract and object-type literals", () => {
        expect(
            isCustomerAcceptanceWorkItem({
                businessObjectType: "CustomerAcceptance",
            }),
        ).toBe(true)
        expect(
            isCustomerAcceptanceWorkItem({
                businessObjectType: "customer_acceptance",
            }),
        ).toBe(true)
        expect(
            isCustomerAcceptanceWorkItem({
                businessObjectType: "ServiceFulfillment",
            }),
        ).toBe(false)
        expect(
            isCustomerAcceptanceWorkItem({
                businessObjectType: "Delivery",
            }),
        ).toBe(false)
        expect(
            isCustomerAcceptanceWorkItem({
                businessObjectType: "PurchaseReceipt",
            }),
        ).toBe(false)
        expect(isCustomerAcceptanceWorkItem(undefined)).toBe(false)
    })
})

describe("isCustomerAcceptanceHandoff", () => {
    it("accepts only posted outcomes that require sales acceptance", () => {
        expect(isCustomerAcceptanceHandoff({ acceptanceRequired: true })).toBe(
            true,
        )
        expect(isCustomerAcceptanceHandoff({ acceptanceRequired: false })).toBe(
            false,
        )
        expect(isCustomerAcceptanceHandoff(undefined)).toBe(false)
    })
})

describe("stripCustomerAcceptanceApprovalField", () => {
    it("drops a stray approval field and leaves the acceptance handoff intact", () => {
        const stripped = stripCustomerAcceptanceApprovalField({
            salesOrderId: "so-1",
            acceptanceRequired: true as const,
            acceptanceNextStep: "请销售在客户验收登记。",
            approval: {
                requirement: "PROCESS_REQUIRED",
                allowed_actions: ["SUBMIT"],
            },
        })
        expect(stripped).toEqual({
            salesOrderId: "so-1",
            acceptanceRequired: true,
            acceptanceNextStep: "请销售在客户验收登记。",
        })
        expect("approval" in stripped).toBe(false)
        expect(
            stripCustomerAcceptanceApprovalField({ salesOrderId: "so-2" }),
        ).toEqual({
            salesOrderId: "so-2",
        })
    })
})

describe("customerAcceptanceActionsExcludeApproval", () => {
    it("accepts customer acceptance business actions and rejects approval entries", () => {
        expect(
            customerAcceptanceActionsExcludeApproval([
                "VIEW_DETAIL",
                "CREATE_ACCEPTANCE",
                "POST_ACCEPTANCE",
                "REVERSE_ACCEPTANCE",
            ]),
        ).toBe(true)
        expect(
            customerAcceptanceActionsExcludeApproval(["APPROVE", "REJECT"]),
        ).toBe(false)
        expect(customerAcceptanceActionsExcludeApproval(["CANCEL"])).toBe(false)
        expect(
            customerAcceptanceActionsExcludeApproval(["UPGRADE_BINDING"]),
        ).toBe(false)
        expect(customerAcceptanceActionsExcludeApproval(["SUBMIT"])).toBe(false)
    })
})

describe("formalFromDelivery customer acceptance handoff", () => {
    it("projects a shipped delivery as an acceptance handoff without an approval zone", () => {
        const outcome = formalFromDelivery(
            {
                ...deliverySeed(),
                approval: {
                    requirement: "PROCESS_REQUIRED",
                    allowed_actions: ["APPROVE"],
                },
            } as BackendDelivery & { approval: unknown },
            {
                type: "WAREHOUSE_SHIP",
                warehouseId: "wh-1",
                warehouseLabel: "中心仓",
                carrier: "顺丰",
                trackingNo: "SF001",
                shippedAt: "2026-08-14T10:00",
                lines: [
                    {
                        salesOrderLineId: "sol-1",
                        stockReservationId: "rsv-1",
                        quantity: "2",
                    },
                ],
            },
            "dl-1",
        )
        expect(outcome.factType).toBe("DELIVERY")
        expect(outcome.acceptanceRequired).toBe(true)
        expect(isCustomerAcceptanceHandoff(outcome)).toBe(true)
        expect("approval" in outcome).toBe(false)
        expect(outcome.salesOrderId).toBe("so-1")
    })
})

describe("customer acceptance source paths omit the approval zone", () => {
    it("keeps customer-acceptance-only files free of approval components", () => {
        expect(
            sourceMentionsApprovalZone(
                readFeature("lib/customer-acceptance-no-approval.ts"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsBpmInternals(
                readFeature("lib/customer-acceptance-no-approval.ts"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsApprovalZone(readFeature("pages/lib/gate-copy.ts")),
        ).toBe(false)
        expect(
            sourceMentionsBpmInternals(readFeature("pages/lib/gate-copy.ts")),
        ).toBe(false)
    })

    it("does not embed the approval zone inside the customer acceptance create result", () => {
        expect(
            sourceMentionsApprovalZone(
                readFeature("pages/components/fulfillment-result-panel.tsx"),
            ),
        ).toBe(false)
        expect(sourceMentionsApprovalZone(readFeature("api/outcomes.ts"))).toBe(
            false,
        )
        const resultPanel = readFeature(
            "pages/components/fulfillment-result-panel.tsx",
        )
        expect(resultPanel).toContain("去登记客户验收")
        expect(resultPanel).toContain("CustomerAcceptance 为 NO_APPROVAL")
    })

    it("routes customer acceptance submit confirmation through the business dialog, not an approval zone", () => {
        const source = [
            readFeature(
                "pages/components/fulfillment-operations-workspace.tsx",
            ),
            readFeature("pages/components/fulfillment-result-panel.tsx"),
        ].join("\n")
        expect(source).toContain("FormalActionConfirmDialog")
        expect(source).toContain("OPERATION_CONFIRM_TITLE")
        expect(sourceMentionsApprovalZone(source)).toBe(false)
        expect(sourceMentionsBpmInternals(source)).toBe(false)
        expect(source).toContain("CustomerAcceptance 为 NO_APPROVAL")
    })

    it("does not dispatch a customer acceptance draft to an approval form", () => {
        const source = readFeature(
            "components/forms/fulfillment-draft-form.tsx",
        )
        expect(source).toContain("CustomerAcceptance 为 NO_APPROVAL")
        expect(source).not.toContain('case "ACCEPTANCE"')
        expect(source).not.toContain("FulfillmentAcceptanceForm")
        expect(sourceMentionsApprovalZone(source)).toBe(false)
        expect(sourceMentionsBpmInternals(source)).toBe(false)
    })
})

describe("fulfillment page customer acceptance proof", () => {
    it("declares NO_APPROVAL and does not wire customer acceptance approval actions", () => {
        expect(FULFILLMENT_CUSTOMER_ACCEPTANCE_APPROVAL_REQUIREMENT).toBe(
            "NO_APPROVAL",
        )
        expect(FULFILLMENT_CUSTOMER_ACCEPTANCE_FORBIDDEN_ACTIONS).toEqual(
            expect.arrayContaining([
                "选择流程",
                "通过",
                "撤回审批",
                "改派当前审批人",
            ]),
        )
        const pageSource = readFileSync(pagePath, "utf8")
        expect(pageSource).toContain("FulfillmentOperationsPage")
        expect(pageSource).toContain("CustomerAcceptance 为 NO_APPROVAL")
        expect(sourceMentionsApprovalZone(pageSource)).toBe(false)
        expect(sourceMentionsBpmInternals(pageSource)).toBe(false)
        for (const label of FULFILLMENT_CUSTOMER_ACCEPTANCE_FORBIDDEN_ACTIONS) {
            expect(pageSource).not.toContain(label)
        }
    })
})
