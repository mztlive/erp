import { readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

import { describe, expect, it } from "vitest"

import { serviceToOperation } from "@/features/fulfillment-operations/api/documents"
import { formalFromService } from "@/features/fulfillment-operations/api/outcomes"
import type { BackendServiceFulfillment } from "@/features/fulfillment-operations/api/documents"
import {
    FULFILLMENT_SERVICE_FULFILLMENT_APPROVAL_REQUIREMENT,
    FULFILLMENT_SERVICE_FULFILLMENT_FORBIDDEN_ACTIONS,
} from "@/app/(workspace)/fulfillment/service-fulfillment-page-proof"
import {
    SERVICE_FULFILLMENT_APPROVAL_REQUIREMENT,
    SERVICE_FULFILLMENT_DOCUMENT_TYPE,
    SERVICE_FULFILLMENT_DTO_HAS_NO_APPROVAL,
    SERVICE_FULFILLMENT_OBJECT_TYPE,
    SERVICE_FULFILLMENT_OPERATION_HAS_NO_APPROVAL,
    SERVICE_FULFILLMENT_OUTCOME_HAS_NO_APPROVAL,
    isServiceFulfillmentOperation,
    isServiceFulfillmentWorkItem,
    serviceFulfillmentActionsExcludeApproval,
    stripServiceFulfillmentApprovalField,
} from "./service-fulfillment-no-approval"

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

function serviceSeed(): BackendServiceFulfillment {
    return {
        id: "sf-1",
        fulfillment_no: "SF-2026-001",
        sales_order_line_id: "sol-1",
        purchase_order_id: "po-1",
        purchase_line_sales_allocation_id: "alloc-1",
        quantity: "2",
        result: "SUCCESS",
        status: "DRAFT",
        occurred_at: 1_700_000_000,
        recorded_at: 1_700_000_000,
        version: 1,
    }
}

describe("SERVICE_FULFILLMENT_DOCUMENT_TYPE", () => {
    it("uses the contract type and does not alias receipts or deliveries", () => {
        expect(SERVICE_FULFILLMENT_DOCUMENT_TYPE).toBe("ServiceFulfillment")
        expect(SERVICE_FULFILLMENT_OBJECT_TYPE).toBe("service_fulfillment")
        expect(SERVICE_FULFILLMENT_APPROVAL_REQUIREMENT).toBe("NO_APPROVAL")
        expect(SERVICE_FULFILLMENT_DOCUMENT_TYPE).not.toBe("PurchaseReceipt")
        expect(SERVICE_FULFILLMENT_DOCUMENT_TYPE).not.toBe("Delivery")
        expect(SERVICE_FULFILLMENT_DOCUMENT_TYPE).not.toBe("ElectronicDelivery")
        expect(SERVICE_FULFILLMENT_DOCUMENT_TYPE).not.toBe("CustomerAcceptance")
        expect(SERVICE_FULFILLMENT_DTO_HAS_NO_APPROVAL).toBe(true)
        expect(SERVICE_FULFILLMENT_OPERATION_HAS_NO_APPROVAL).toBe(true)
        expect(SERVICE_FULFILLMENT_OUTCOME_HAS_NO_APPROVAL).toBe(true)
    })
})

describe("isServiceFulfillmentWorkItem", () => {
    it("accepts only the contract and object-type literals", () => {
        expect(
            isServiceFulfillmentWorkItem({
                businessObjectType: "ServiceFulfillment",
            }),
        ).toBe(true)
        expect(
            isServiceFulfillmentWorkItem({
                businessObjectType: "service_fulfillment",
            }),
        ).toBe(true)
        expect(
            isServiceFulfillmentWorkItem({
                businessObjectType: "PurchaseReceipt",
            }),
        ).toBe(false)
        expect(
            isServiceFulfillmentWorkItem({
                businessObjectType: "Delivery",
            }),
        ).toBe(false)
        expect(
            isServiceFulfillmentWorkItem({
                businessObjectType: "ElectronicDelivery",
            }),
        ).toBe(false)
        expect(isServiceFulfillmentWorkItem(undefined)).toBe(false)
    })
})

describe("isServiceFulfillmentOperation", () => {
    it("accepts SERVICE operations and SERVICE_FULFILLMENT facts", () => {
        expect(
            isServiceFulfillmentOperation({ operationType: "SERVICE" }),
        ).toBe(true)
        expect(
            isServiceFulfillmentOperation({
                factType: "SERVICE_FULFILLMENT",
            }),
        ).toBe(true)
        expect(
            isServiceFulfillmentOperation({ operationType: "ELECTRONIC" }),
        ).toBe(false)
        expect(
            isServiceFulfillmentOperation({ operationType: "WAREHOUSE_SHIP" }),
        ).toBe(false)
        expect(
            isServiceFulfillmentOperation({ operationType: "RECEIPT" }),
        ).toBe(false)
        expect(isServiceFulfillmentOperation(undefined)).toBe(false)
    })
})

describe("stripServiceFulfillmentApprovalField", () => {
    it("drops a stray approval field and leaves service fulfillment facts intact", () => {
        const stripped = stripServiceFulfillmentApprovalField({
            id: "sf-1",
            fulfillment_no: "SF-1",
            approval: {
                requirement: "PROCESS_REQUIRED",
                allowed_actions: ["SUBMIT"],
            },
        })
        expect(stripped).toEqual({ id: "sf-1", fulfillment_no: "SF-1" })
        expect("approval" in stripped).toBe(false)
        expect(stripServiceFulfillmentApprovalField({ id: "sf-2" })).toEqual({
            id: "sf-2",
        })
    })
})

describe("serviceFulfillmentActionsExcludeApproval", () => {
    it("accepts service fulfillment business actions and rejects approval entries", () => {
        expect(
            serviceFulfillmentActionsExcludeApproval([
                "VIEW_DETAIL",
                "POST",
                "CONFIRM",
            ]),
        ).toBe(true)
        expect(
            serviceFulfillmentActionsExcludeApproval(["APPROVE", "REJECT"]),
        ).toBe(false)
        expect(serviceFulfillmentActionsExcludeApproval(["CANCEL"])).toBe(false)
        expect(
            serviceFulfillmentActionsExcludeApproval(["UPGRADE_BINDING"]),
        ).toBe(false)
        expect(serviceFulfillmentActionsExcludeApproval(["SUBMIT"])).toBe(false)
    })
})

describe("serviceToOperation", () => {
    it("maps a service fulfillment without an approval projection", () => {
        const operation = serviceToOperation(serviceSeed())
        expect(operation.operationType).toBe("SERVICE")
        expect(operation.summary).toBe("SF-2026-001")
        expect("approval" in operation).toBe(false)
        expect(
            serviceFulfillmentActionsExcludeApproval(
                operation.actionBlockers.map((blocker) => blocker.action),
            ),
        ).toBe(true)
    })

    it("strips a stray approval field instead of rendering a binding", () => {
        const operation = serviceToOperation({
            ...serviceSeed(),
            approval: {
                requirement: "PROCESS_REQUIRED",
                allowed_actions: ["SUBMIT", "APPROVE"],
            },
        } as BackendServiceFulfillment & { approval: unknown })
        expect("approval" in operation).toBe(false)
        expect(operation.operationType).toBe("SERVICE")
        expect(operation.summary).toBe("SF-2026-001")
    })
})

describe("formalFromService", () => {
    it("projects a confirmed service fulfillment without an approval zone", () => {
        const outcome = formalFromService(
            {
                ...serviceSeed(),
                status: "CONFIRMED",
                result: "SUCCESS",
                occurred_at: 1_700_000_100,
                approval: {
                    requirement: "PROCESS_REQUIRED",
                    allowed_actions: ["APPROVE"],
                },
            } as BackendServiceFulfillment & { approval: unknown },
            {
                type: "SERVICE",
                startedAt: "2026-08-14T09:00",
                endedAt: "2026-08-14T11:00",
                serviceLocation: "客户现场",
                result: "SUCCESS",
                completionNote: "已完成安装",
                lines: [
                    {
                        salesOrderLineId: "sol-1",
                        purchaseLineSalesAllocationId: "alloc-1",
                        quantity: "2",
                    },
                ],
            },
            "sf-1",
        )
        expect(outcome.factType).toBe("SERVICE_FULFILLMENT")
        expect(outcome.factNo).toBe("SF-2026-001")
        expect(outcome.operationType).toBe("SERVICE")
        expect("approval" in outcome).toBe(false)
    })

    it("projects a failed service fulfillment without an approval zone", () => {
        const outcome = formalFromService(
            {
                ...serviceSeed(),
                status: "FAILED",
                result: "FAILED",
            },
            {
                type: "SERVICE",
                startedAt: "2026-08-14T09:00",
                endedAt: "2026-08-14T11:00",
                serviceLocation: "客户现场",
                result: "FAILED",
                completionNote: "现场无法施工",
                lines: [
                    {
                        salesOrderLineId: "sol-1",
                        purchaseLineSalesAllocationId: "alloc-1",
                        quantity: "2",
                    },
                ],
            },
            "sf-1",
        )
        expect(outcome.formalStatus).toBe("FAILED")
        expect(outcome.acceptanceRequired).toBe(false)
        expect("approval" in outcome).toBe(false)
    })
})

describe("service fulfillment source paths omit the approval zone", () => {
    it("keeps service-fulfillment-only files free of approval components", () => {
        expect(
            sourceMentionsApprovalZone(
                readFeature("components/forms/fulfillment-service-form.tsx"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsApprovalZone(
                readFeature("lib/service-fulfillment-no-approval.ts"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsBpmInternals(
                readFeature("lib/service-fulfillment-no-approval.ts"),
            ),
        ).toBe(false)
        expect(
            sourceMentionsBpmInternals(
                readFeature("components/forms/fulfillment-service-form.tsx"),
            ),
        ).toBe(false)
    })

    it("does not embed the approval zone inside the service fulfillment create result", () => {
        expect(
            sourceMentionsApprovalZone(
                readFeature("pages/components/fulfillment-result-panel.tsx"),
            ),
        ).toBe(false)
        expect(sourceMentionsApprovalZone(readFeature("api/outcomes.ts"))).toBe(
            false,
        )
    })

    it("routes service fulfillment submit confirmation through the business dialog, not an approval zone", () => {
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
        expect(source).toContain("ServiceFulfillment 为 NO_APPROVAL")
    })

    it("does not dispatch SERVICE drafts to an approval form", () => {
        const source = readFeature(
            "components/forms/fulfillment-draft-form.tsx",
        )
        const start = source.indexOf('case "SERVICE"')
        expect(start).toBeGreaterThan(-1)
        const serviceBranch = source.slice(start)
        expect(serviceBranch).toContain("FulfillmentServiceForm")
        expect(sourceMentionsApprovalZone(serviceBranch)).toBe(false)
        expect(sourceMentionsBpmInternals(serviceBranch)).toBe(false)
    })
})

describe("fulfillment page service fulfillment proof", () => {
    it("declares NO_APPROVAL and does not wire service fulfillment approval actions", () => {
        expect(FULFILLMENT_SERVICE_FULFILLMENT_APPROVAL_REQUIREMENT).toBe(
            "NO_APPROVAL",
        )
        expect(FULFILLMENT_SERVICE_FULFILLMENT_FORBIDDEN_ACTIONS).toEqual(
            expect.arrayContaining([
                "选择流程",
                "通过",
                "撤回审批",
                "改派当前审批人",
            ]),
        )
        const pageSource = readFileSync(pagePath, "utf8")
        expect(pageSource).toContain("FulfillmentOperationsPage")
        expect(pageSource).toContain("ServiceFulfillment 为 NO_APPROVAL")
        expect(sourceMentionsApprovalZone(pageSource)).toBe(false)
        expect(sourceMentionsBpmInternals(pageSource)).toBe(false)
        for (const label of FULFILLMENT_SERVICE_FULFILLMENT_FORBIDDEN_ACTIONS) {
            expect(pageSource).not.toContain(label)
        }
    })
})
