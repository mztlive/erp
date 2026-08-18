import { describe, it, expect } from "vitest"

import { deriveSalesOrderDetailState } from "@/features/sales-orders/lib/sales-order-detail-derived"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import type { ProcurementRejectionResolution } from "@/features/sales-orders/types"

function makeRejection(
    allowedActions: ProcurementRejectionResolution["allowedActions"] = [],
): ProcurementRejectionResolution {
    return {
        rejectedProcurementConfirmationId: "pc-1",
        rejectedProcurementWorkItemId: "",
        rejectedSubmissionId: "sub-1",
        rejectedSubmissionNo: 2,
        rejectedSubjectHash: "sub-1",
        rejectReasonCode: "CANNOT_FULFILL",
        rejectComment: "无法交付",
        rejectedByLabel: "采购",
        rejectedAt: "2026-08-01 10:00",
        reviewStatus: "REJECTED",
        draftDifference: {
            changedItemOrService: false,
            changedSalesPrice: false,
            commercialTermsUnchanged: true,
            diffSummary: [],
        },
        fixedResolutions: [
            "RESUBMIT_CHANGED_TERMS",
            "REQUEST_LOW_MARGIN_ACCEPTANCE",
            "VOID_AFTER_REJECTION",
        ],
        allowedActions,
        actionBlockers: [],
    }
}

function makeOrder(
    overrides: Partial<SalesOrderDetailView> = {},
): SalesOrderDetailView {
    return {
        id: "so-1",
        documentNumber: "XS-2026-001",
        customerName: "客户甲",
        contractId: "",
        contractNumber: "",
        contractCompanyName: "",
        contractRevisionLabel: "",
        nature: "physical_service",
        originSystem: "erp",
        primaryStatus: { code: "effective", label: "已生效", tone: "success" },
        fulfillment: { label: "未开始", tone: "neutral" },
        collection: { label: "未收", tone: "neutral" },
        invoicing: { label: "未开", tone: "neutral" },
        amountGross: "0.00",
        amountNet: "0.00",
        taxAmount: "0.00",
        receivedAmount: "0.00",
        invoicedAmount: "0.00",
        ownerName: "张三",
        submittedAt: "",
        welfareScene: "",
        version: 3,
        lockVersion: 3,
        settlementEntity: "",
        sellerEntity: "",
        paymentTerms: "",
        fulfillmentDeadline: "",
        lineItems: [],
        related: {
            purchaseOrders: 0,
            fulfillments: 0,
            receipts: 0,
            invoices: 0,
        },
        closeEligibility: {
            fulfillmentComplete: false,
            receivableSettled: false,
            invoiceComplete: false,
            eligibleToClose: false,
            blockers: [],
            note: "",
        },
        natureLocked: true,
        commercialReadOnly: false,
        revisions: [],
        procurementRejection: null,
        activeCardSalesApproval: null,
        activeLowMarginManagerConfirmation: null,
        cardApprovalProjectionBlocker: null,
        activeChangeOrder: null,
        allowedActions: [],
        actionBlockers: [],
        acceptance: null,
        permissionVersion: "1",
        sourceAsOf: "",
        queriedAt: "",
        ...overrides,
    } as unknown as SalesOrderDetailView
}

const baseInput = {
    section: undefined,
    pageMode: null,
    fromWorkspace: null,
    returnTo: null,
}

describe("deriveSalesOrderDetailState", () => {
    it("derives overview defaults for a plain effective order", () => {
        const state = deriveSalesOrderDetailState(makeOrder(), baseInput)

        expect(state.navSection).toBe("overview")
        expect(state.acceptanceExpanded).toBe(false)
        expect(state.showApproval).toBe(false)
        expect(state.showEditor).toBe(false)
        expect(state.canAccept).toBe(false)
        expect(state.canResubmit).toBe(false)
        expect(state.canVoid).toBe(false)
        expect(state.openRejection).toBe(false)
        expect(state.focusTask).toBeNull()
        expect(state.actionableFocusTask).toBeNull()
        expect(state.bannerJump).toBe(false)
        expect(state.visibleNav.map((item) => item.id)).toEqual([
            "overview",
            "fulfillment",
            "receivable",
            "versions",
        ])
    })

    it("maps acceptance section onto the fulfillment tab", () => {
        const state = deriveSalesOrderDetailState(makeOrder(), {
            ...baseInput,
            section: "acceptance",
        })

        expect(state.navSection).toBe("fulfillment")
        expect(state.acceptanceExpanded).toBe(true)
    })

    it("respects the workspace the page was opened from", () => {
        expect(
            deriveSalesOrderDetailState(makeOrder(), {
                ...baseInput,
                fromWorkspace: "W09",
                returnTo: "/workspace/tasks",
            }).navSection,
        ).toBe("fulfillment")
        expect(
            deriveSalesOrderDetailState(makeOrder(), {
                ...baseInput,
                fromWorkspace: "W13",
            }).navSection,
        ).toBe("receivable")
    })

    it("shows the collaboration tab only for card orders", () => {
        const card = deriveSalesOrderDetailState(
            makeOrder({ nature: "card_voucher" }),
            { ...baseInput, section: "collaboration" },
        )
        expect(card.navSection).toBe("collaboration")
        expect(card.visibleNav.map((item) => item.id)).toContain(
            "collaboration",
        )

        const physical = deriveSalesOrderDetailState(makeOrder(), {
            ...baseInput,
            section: "collaboration",
        })
        expect(physical.navSection).toBe("overview")
    })

    it("computes acceptance eligibility from nature and allowed actions", () => {
        const physical = deriveSalesOrderDetailState(
            makeOrder({ allowedActions: ["REGISTER_ACCEPTANCE"] }),
            baseInput,
        )
        expect(physical.canAccept).toBe(true)
        expect(physical.focusTask?.id).toBe("acceptance")

        const card = deriveSalesOrderDetailState(
            makeOrder({
                nature: "card_voucher",
                allowedActions: ["REGISTER_ACCEPTANCE"],
            }),
            baseInput,
        )
        expect(card.canAccept).toBe(false)
    })

    it("shows the approval panel only inside the approval section", () => {
        const order = makeOrder({
            nature: "card_voucher",
            activeCardSalesApproval: {
                approvalInstanceId: "ai-1",
                instanceVersion: "1",
                approvalStepInstanceId: "si-1",
                stepVersion: "1",
                processingState: "READY",
                subjectVersion: "1",
                salesOrderSubmissionId: "sub-1",
                submissionNo: 1,
                frozenSubmissionSummary: "摘要",
                expectedReviewStatus: "PENDING_SALES_LEAD",
                allowedActions: ["APPROVE"],
                actionBlockers: [],
                workItemId: "wi-1",
                workItemType: "CARD_SALES_MANAGER_APPROVAL",
                taskVersion: "1",
                workItemStatus: "OPEN",
                assignmentMode: "DIRECT",
            },
        })

        expect(
            deriveSalesOrderDetailState(order, {
                ...baseInput,
                section: "approval",
            }).showApproval,
        ).toBe(true)
        expect(deriveSalesOrderDetailState(order, baseInput).showApproval).toBe(
            false,
        )
        expect(
            deriveSalesOrderDetailState(order, baseInput).focusTask?.id,
        ).toBe("approval")
    })

    it("opens the editor for drafts without an explicit mode", () => {
        const draft = makeOrder({
            primaryStatus: { code: "draft", label: "草稿", tone: "neutral" },
        })
        expect(deriveSalesOrderDetailState(draft, baseInput).showEditor).toBe(
            true,
        )
    })

    it("requires mode=edit plus resubmit permission for rejected orders", () => {
        const rejection = makeRejection(["RESUBMIT_CHANGED_TERMS"])
        const order = makeOrder({ procurementRejection: rejection })

        expect(
            deriveSalesOrderDetailState(order, {
                ...baseInput,
                pageMode: "edit",
            }).showEditor,
        ).toBe(true)
        expect(deriveSalesOrderDetailState(order, baseInput).showEditor).toBe(
            false,
        )

        const withoutResubmit = makeOrder({
            procurementRejection: makeRejection([
                "REQUEST_LOW_MARGIN_ACCEPTANCE",
            ]),
        })
        expect(
            deriveSalesOrderDetailState(withoutResubmit, {
                ...baseInput,
                pageMode: "edit",
            }).showEditor,
        ).toBe(false)
    })

    it("does not treat procurement rejection as an independent work surface for goods orders", () => {
        const rejection = makeRejection([
            "RESUBMIT_CHANGED_TERMS",
            "VOID_AFTER_REJECTION",
            "REQUEST_LOW_MARGIN_ACCEPTANCE",
        ])
        const state = deriveSalesOrderDetailState(
            makeOrder({ procurementRejection: rejection }),
            baseInput,
        )

        expect(state.openRejection).toBe(true)
        expect(state.canResubmit).toBe(true)
        expect(state.canVoid).toBe(true)
        expect(state.canRequestLowMargin).toBe(true)
        expect(state.focusTask?.id).not.toBe("procurement-rejection")
        expect(state.hasPrimaryTaskAction).toBe(false)
    })

    it("embeds the goods-order approval area from the server projection", () => {
        const state = deriveSalesOrderDetailState(
            makeOrder({
                nature: "physical_service",
                approval: {
                    requirement: "PROCESS_REQUIRED",
                    definition: {
                        id: "def-1",
                        name: "实物销售审批",
                        version: 2,
                        nodes: [
                            {
                                key: "n1",
                                name: "销售审核",
                                assigneeName: "张三",
                            },
                        ],
                        publishedNodes: [],
                    },
                    recentHistory: [],
                    historyHasMore: false,
                    allowedActions: ["SUBMIT"],
                },
            }),
            baseInput,
        )

        expect(state.showApproval).toBe(true)
        expect(state.focusTask).toBeNull()
    })

    it("surfaces a change order as a versions focus task", () => {
        const state = deriveSalesOrderDetailState(
            makeOrder({
                activeChangeOrder: {
                    id: "sc-1",
                    statusLabel: "待采购履约影响确认",
                    statusTone: "warning",
                    baseRevisionNo: 3,
                    createdAt: "2026-08-14T00:00:00.000Z",
                    impactPath: "procurement",
                },
            }),
            baseInput,
        )

        expect(state.focusTask?.id).toBe("versions")
        expect(state.bannerJump).toBe(true)
    })

    it("builds the encoded self return href with section and context", () => {
        const state = deriveSalesOrderDetailState(makeOrder(), {
            ...baseInput,
            section: "receivable",
            returnTo: "/workspace/tasks?x=1",
            fromWorkspace: "W11",
        })

        expect(state.returnSection).toBe("receivable")
        expect(state.selfReturn).toBe(
            encodeURIComponent(
                "/sales/orders/so-1?section=receivable&returnTo=%2Fworkspace%2Ftasks%3Fx%3D1&from=W11",
            ),
        )
    })

    it("exposes the start-change blocker from action blockers", () => {
        const state = deriveSalesOrderDetailState(
            makeOrder({
                allowedActions: ["START_SALES_CHANGE"],
                actionBlockers: [
                    { action: "START_SALES_CHANGE", reason: "本单已生效" },
                ],
            }),
            baseInput,
        )

        expect(state.canStartChange).toBe(true)
        expect(state.changeBlocker?.reason).toBe("本单已生效")
    })

    it("keeps the acceptance jump on the primary action, not the banner", () => {
        const order = makeOrder({ allowedActions: ["REGISTER_ACCEPTANCE"] })

        const collapsed = deriveSalesOrderDetailState(order, baseInput)
        expect(collapsed.hasPrimaryTaskAction).toBe(true)
        expect(collapsed.bannerJump).toBe(false)

        const expanded = deriveSalesOrderDetailState(order, {
            ...baseInput,
            section: "acceptance",
        })
        expect(expanded.hasPrimaryTaskAction).toBe(true)
        expect(expanded.bannerJump).toBe(false)
    })
})
