import { describe, expect, it } from "vitest"

import {
    mapSalesOrderApproval,
    mergeSalesOrderAllowedActions,
    readSalesOrderApprovalResponsibility,
    salesOrderApprovalPhase,
    salesOrderMarginRiskHint,
} from "./sales-order-approval"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"

const binding: DocumentApprovalView = {
    requirement: "PROCESS_REQUIRED",
    definition: {
        id: "def-1",
        name: "实物销售审批",
        version: 2,
        nodes: [{ key: "n1", name: "销售审核", assigneeName: "张三" }],
        publishedNodes: [],
    },
    recentHistory: [],
    historyHasMore: false,
    allowedActions: ["SUBMIT"],
}

describe("salesOrderApprovalPhase", () => {
    it("keeps unsubmitted documents on the binding card", () => {
        expect(salesOrderApprovalPhase(binding, "draft")).toBe("draft")
        expect(salesOrderApprovalPhase(undefined, undefined)).toBe("draft")
    })

    it("switches to runtime when an instance exists", () => {
        expect(
            salesOrderApprovalPhase(
                {
                    ...binding,
                    instance: {
                        id: "inst-1",
                        status: "RUNNING",
                        currentRoundNo: 1,
                    },
                },
                "draft",
            ),
        ).toBe("runtime")
        expect(salesOrderApprovalPhase(binding, "in_approval")).toBe("runtime")
    })
})

describe("mapSalesOrderApproval", () => {
    it("maps the created binding without turning it into a work item", () => {
        const view = mapSalesOrderApproval({
            requirement: "PROCESS_REQUIRED",
            definition: {
                id: "def-1",
                name: "实物销售审批",
                version: 2,
                nodes: [{ key: "n1", name: "销售审核", assignee_name: "张三" }],
            },
            instance: null,
            recent_history: [],
            allowed_actions: ["SUBMIT", "UPGRADE_BINDING"],
        })
        expect(view?.instance).toBeUndefined()
        expect(view?.definition?.name).toBe("实物销售审批")
        expect(view?.definition?.nodes[0]?.assigneeName).toBe("张三")
        expect(view?.allowedActions).toEqual(["SUBMIT", "UPGRADE_BINDING"])
    })

    it("returns undefined when the document has no approval projection", () => {
        expect(mapSalesOrderApproval(null)).toBeUndefined()
        expect(mapSalesOrderApproval(undefined)).toBeUndefined()
    })
})

describe("mergeSalesOrderAllowedActions", () => {
    it("unions server facts and drops unknown codes", () => {
        expect(
            mergeSalesOrderAllowedActions(
                ["CANCEL"],
                ["APPROVE", "START_PROCESSING"],
            ),
        ).toEqual(["CANCEL", "APPROVE"])
    })
})

describe("readSalesOrderApprovalResponsibility", () => {
    it("reads only instance fields and does not fall back to the first definition node", () => {
        expect(
            readSalesOrderApprovalResponsibility({
                ...binding,
                instance: {
                    id: "inst-1",
                    status: "RUNNING",
                    currentRoundNo: 1,
                    currentNodeName: "采购确认",
                    currentAssigneeName: "李四",
                },
            }),
        ).toEqual({
            nextResponsible: "李四",
            currentNodeLabel: "采购确认",
        })
        expect(readSalesOrderApprovalResponsibility(binding)).toEqual({
            nextResponsible: undefined,
            currentNodeLabel: undefined,
        })
    })
})

describe("salesOrderMarginRiskHint", () => {
    it("is read-only and never implies a blocking work surface", () => {
        expect(
            salesOrderMarginRiskHint({
                procurementRejection: {
                    rejectedProcurementConfirmationId: "pc-1",
                    rejectedProcurementWorkItemId: "",
                    rejectedSubmissionId: "sub-1",
                    rejectedSubmissionNo: 1,
                    rejectedSubjectHash: "sub-1",
                    rejectReasonCode: "MARGIN_TOO_LOW",
                    rejectComment: "",
                    rejectedByLabel: "",
                    rejectedAt: "",
                    reviewStatus: "REJECTED",
                    draftDifference: {
                        changedItemOrService: false,
                        changedSalesPrice: false,
                        commercialTermsUnchanged: true,
                        diffSummary: [],
                    },
                    estimatedMarginPercent: "3.20",
                    fixedResolutions: [
                        "RESUBMIT_CHANGED_TERMS",
                        "REQUEST_LOW_MARGIN_ACCEPTANCE",
                        "VOID_AFTER_REJECTION",
                    ],
                    allowedActions: ["REQUEST_LOW_MARGIN_ACCEPTANCE"],
                    actionBlockers: [],
                },
                activeLowMarginManagerConfirmation: null,
            }),
        ).toBe("预计毛利 3.20%。毛利风险仅供参考，不阻断提交或审批决定。")
        expect(
            salesOrderMarginRiskHint({
                procurementRejection: null,
                activeLowMarginManagerConfirmation: null,
            }),
        ).toBeNull()
    })
})
