import { describe, it, expect, vi, beforeEach } from "vitest"
import { act, waitFor } from "@testing-library/react"

import * as salesOrdersApi from "@/features/sales-orders/api/sales-orders"
import {
    salesOrderKeys,
    useAdjustProcurementRejectionDraftMutation,
    useCancelCardSalesApprovalMutation,
    useCreateSalesOrderExportJobMutation,
    useCreateSalesOrderMutation,
    useResolveProcurementRejectionMutation,
    useSalesChangeReviewDecisionMutation,
    useSalesOrderDetailQuery,
    useSalesOrderDraftResumeQuery,
    useSalesOrdersQuery,
    useSaveSalesOrderDraftMutation,
    useStartSalesChangeOrderMutation,
    useSubmitCardSalesApprovalDecisionMutation,
    useSubmitSalesOrderMutation,
} from "@/features/sales-orders/hooks/queries"
import type { CreateSalesOrderResult } from "@/features/sales-orders/types"
import type {
    SalesOrderDetailView,
    SalesOrdersListQuery,
} from "@/features/sales-orders/api/sales-orders"
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"

vi.mock("@/features/sales-orders/api/sales-orders", () => ({
    adjustProcurementRejectionDraft: vi.fn(),
    cancelCardSalesApproval: vi.fn(),
    createSalesOrder: vi.fn(),
    createSalesOrderExportJob: vi.fn(),
    fetchSalesOrderDetail: vi.fn(),
    fetchSalesOrderDraftForResume: vi.fn(),
    fetchSalesOrders: vi.fn(),
    resolveProcurementRejection: vi.fn(),
    saveSalesOrderDraft: vi.fn(),
    startSalesChangeOrder: vi.fn(),
    submitCardSalesApprovalDecision: vi.fn(),
    submitSalesChangeReviewDecision: vi.fn(),
    submitSalesOrder: vi.fn(),
}))

const mockedApi = vi.mocked(salesOrdersApi)

const baseQuery: SalesOrdersListQuery = {
    page: 1,
    pageSize: 20,
    nature: "all",
    origin: "all",
}

const emptyListView = (page: number) => ({
    items: [],
    total: 0,
    page,
    pageSize: 20,
    queriedAt: "2026-08-14T00:00:00.000Z",
})

const createdResult: CreateSalesOrderResult = {
    salesOrderId: "so-1",
    documentNumber: "SO-1",
    statusLabel: "草稿",
    createdAt: "2026-08-14T00:00:00.000Z",
    reference: "SO-CREATE-SO-1",
}

const minimalDetail = { id: "so-1" } as unknown as SalesOrderDetailView

const saveDraftInput = {
    salesOrderId: "so-1",
    version: 2,
    contract: { contractId: "ct-1", requestedContractRevisionId: "rv-1" },
    nature: "physical_service" as const,
    ownerUserId: "u1",
    ownerName: "销售",
    welfareScene: "annual",
    paymentTerms: "CONTRACT",
    fulfillmentDeadline: "2026-09-01",
    targetMallId: "",
    receivableDueDate: "",
    taxRatePercent: "13",
    remark: "",
    lineItems: [],
}

describe("salesOrderKeys", () => {
    it("structures keys by resource layer", () => {
        expect(salesOrderKeys.all).toEqual(["sales-orders"])
        expect(salesOrderKeys.list(baseQuery)).toEqual([
            "sales-orders",
            "list",
            baseQuery,
        ])
        expect(salesOrderKeys.detail("so-1")).toEqual([
            "sales-orders",
            "detail",
            "so-1",
        ])
        expect(salesOrderKeys.acceptanceRoot("so-1")).toEqual([
            "sales-orders",
            "acceptance",
            "so-1",
        ])
        expect(
            salesOrderKeys.acceptance("so-1", {
                remainingOnly: true,
                workItemId: null,
            }),
        ).toEqual([
            "sales-orders",
            "acceptance",
            "so-1",
            { remainingOnly: true, workItemId: null },
        ])
    })

    it("produces stable keys for structurally identical queries", () => {
        const a = salesOrderKeys.list(baseQuery)
        const b = salesOrderKeys.list({ ...baseQuery })
        expect(a).toEqual(b)
    })
})

describe("useSalesOrdersQuery", () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it("fetches with the given query and resolves the view", async () => {
        mockedApi.fetchSalesOrders.mockResolvedValue(emptyListView(1))

        const { result } = renderHookWithProviders(() =>
            useSalesOrdersQuery(baseQuery),
        )

        expect(result.current.isPending).toBe(true)
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(mockedApi.fetchSalesOrders).toHaveBeenCalledWith(baseQuery)
        expect(result.current.data).toEqual(emptyListView(1))
    })

    it("propagates error responses", async () => {
        mockedApi.fetchSalesOrders.mockRejectedValue(new Error("后端拒绝请求"))

        const { result } = renderHookWithProviders(() =>
            useSalesOrdersQuery(baseQuery),
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })

    it("skips fetching when disabled", async () => {
        mockedApi.fetchSalesOrders.mockResolvedValue(emptyListView(1))

        const { result } = renderHookWithProviders(() =>
            useSalesOrdersQuery(baseQuery, false),
        )

        expect(result.current.isPending).toBe(true)
        expect(result.current.fetchStatus).toBe("idle")
        expect(mockedApi.fetchSalesOrders).not.toHaveBeenCalled()
    })
})

describe("useSalesOrderDetailQuery", () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it("fetches the detail by id and resolves data", async () => {
        mockedApi.fetchSalesOrderDetail.mockResolvedValue(minimalDetail)

        const { result } = renderHookWithProviders(() =>
            useSalesOrderDetailQuery("so-1"),
        )

        expect(result.current.isPending).toBe(true)
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(mockedApi.fetchSalesOrderDetail).toHaveBeenCalledWith("so-1")
        expect(result.current.data).toBe(minimalDetail)
    })

    it("treats a missing order as empty data", async () => {
        mockedApi.fetchSalesOrderDetail.mockResolvedValue(null)

        const { result } = renderHookWithProviders(() =>
            useSalesOrderDetailQuery("so-missing"),
        )

        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data).toBeNull()
    })
})

describe("useSalesOrderDraftResumeQuery", () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it("does not fetch for an empty id", async () => {
        mockedApi.fetchSalesOrderDraftForResume.mockResolvedValue(null)

        const { result } = renderHookWithProviders(() =>
            useSalesOrderDraftResumeQuery(""),
        )

        expect(result.current.fetchStatus).toBe("idle")
        expect(mockedApi.fetchSalesOrderDraftForResume).not.toHaveBeenCalled()
    })

    it("fetches the resume data for a non-empty id", async () => {
        const resumeData = {
            salesOrderId: "so-1",
            documentNumber: "SO-1",
            version: 3,
            contractId: "ct-1",
            nature: "physical_service" as const,
            welfareScene: "",
            paymentTerms: "CONTRACT",
            fulfillmentDeadline: "2026-09-01",
            targetMallId: "",
            receivableDueDate: "",
            taxRatePercent: "13",
            remark: "",
            lineItems: [],
        }
        mockedApi.fetchSalesOrderDraftForResume.mockResolvedValue(resumeData)

        const { result } = renderHookWithProviders(() =>
            useSalesOrderDraftResumeQuery("so-1"),
        )

        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data).toEqual(resumeData)
    })
})

describe("sales order mutations", () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it("useCreateSalesOrderMutation wires createSalesOrder and refreshes contracts", async () => {
        mockedApi.createSalesOrder.mockResolvedValue(createdResult)
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, "invalidateQueries")

        const { result } = renderHookWithProviders(
            () => useCreateSalesOrderMutation(),
            { queryClient: client },
        )

        const input = {
            orderNo: "SO-1",
            contract: {
                contractId: "ct-1",
                requestedContractRevisionId: "rv-1",
            },
            nature: "physical_service" as const,
            ownerUserId: "u1",
            ownerName: "销售",
            welfareScene: "annual",
            paymentTerms: "CONTRACT",
            fulfillmentDeadline: "2026-09-01",
            targetMallId: "",
            receivableDueDate: "",
            taxRatePercent: "13",
            remark: "",
            lineItems: [],
            intent: "SUBMIT" as const,
            idempotencyKey: "key-1",
        }
        await act(async () => {
            const data = await result.current.mutateAsync(input)
            expect(data).toEqual(createdResult)
        })

        expect(mockedApi.createSalesOrder).toHaveBeenCalledWith(
            input,
            expect.anything(),
        )
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: salesOrderKeys.all,
        })
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: salesOrderKeys.detail("so-1"),
        })
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: ["contracts"],
        })
    })

    it("useSaveSalesOrderDraftMutation invalidates the edited order detail", async () => {
        mockedApi.saveSalesOrderDraft.mockResolvedValue({ version: 3 })
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, "invalidateQueries")

        const { result } = renderHookWithProviders(
            () => useSaveSalesOrderDraftMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync(saveDraftInput)
        })

        expect(mockedApi.saveSalesOrderDraft).toHaveBeenCalledWith(
            saveDraftInput,
            expect.anything(),
        )
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: salesOrderKeys.detail("so-1"),
        })
    })

    it("useSubmitSalesOrderMutation wires submit and invalidates list and detail", async () => {
        mockedApi.submitSalesOrder.mockResolvedValue({
            salesOrderId: "so-1",
        })
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, "invalidateQueries")

        const { result } = renderHookWithProviders(
            () => useSubmitSalesOrderMutation(),
            { queryClient: client },
        )

        const input = {
            salesOrderId: "so-1",
            version: 4,
            idempotencyKey: "key-2",
        }
        await act(async () => {
            await result.current.mutateAsync(input)
        })

        expect(mockedApi.submitSalesOrder).toHaveBeenCalledWith(
            input,
            expect.anything(),
        )
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: salesOrderKeys.all,
        })
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: salesOrderKeys.detail("so-1"),
        })
    })

    it("useAdjustProcurementRejectionDraftMutation invalidates the order detail", async () => {
        mockedApi.adjustProcurementRejectionDraft.mockResolvedValue({
            ok: true,
        })
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, "invalidateQueries")

        const { result } = renderHookWithProviders(
            () => useAdjustProcurementRejectionDraftMutation(),
            { queryClient: client },
        )

        const input = {
            salesOrderId: "so-1",
            unitPriceGross: "99.0000",
            note: "改价",
        }
        await act(async () => {
            await result.current.mutateAsync(input)
        })

        expect(mockedApi.adjustProcurementRejectionDraft).toHaveBeenCalledWith(
            input,
            expect.anything(),
        )
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: salesOrderKeys.detail("so-1"),
        })
    })

    it("useResolveProcurementRejectionMutation invalidates detail and list", async () => {
        mockedApi.resolveProcurementRejection.mockResolvedValue({
            outcome: "VOIDED_AFTER_PROCUREMENT_REJECTION",
            reference: "wf-1",
            detail: "已作废",
            reviewStatus: "VOIDED",
            primaryStatusLabel: "已作废",
        })
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, "invalidateQueries")

        const { result } = renderHookWithProviders(
            () => useResolveProcurementRejectionMutation(),
            { queryClient: client },
        )

        const input = {
            salesOrderId: "so-1",
            action: "VOID_AFTER_REJECTION" as const,
            voidReasonCode: "CLIENT_CANCEL",
            comment: "客户取消",
            rejectedProcurementConfirmationId: "pc-1",
            rejectedSubmissionId: "sub-1",
            expectedSalesOrderLockVersion: 3,
            idempotencyKey: "key-3",
        }
        await act(async () => {
            await result.current.mutateAsync(input)
        })

        expect(mockedApi.resolveProcurementRejection).toHaveBeenCalledWith(
            input,
            expect.anything(),
        )
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: salesOrderKeys.detail("so-1"),
        })
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: salesOrderKeys.all,
        })
    })

    it("useStartSalesChangeOrderMutation invalidates detail and list", async () => {
        mockedApi.startSalesChangeOrder.mockResolvedValue({
            id: "co-1",
            statusLabel: "待复核",
            statusTone: "warning",
            baseRevisionNo: 2,
            createdAt: "2026-08-14",
            impactPath: "procurement",
        })
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, "invalidateQueries")

        const { result } = renderHookWithProviders(
            () => useStartSalesChangeOrderMutation(),
            { queryClient: client },
        )

        const input = {
            salesOrderId: "so-1",
            baseRevisionNo: 2,
            nature: "physical_service" as const,
            command: {},
            idempotencyKey: "key-4",
        }
        await act(async () => {
            await result.current.mutateAsync(input)
        })

        expect(mockedApi.startSalesChangeOrder).toHaveBeenCalledWith(
            input,
            expect.anything(),
        )
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: salesOrderKeys.detail("so-1"),
        })
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: salesOrderKeys.all,
        })
    })

    it("useSalesChangeReviewDecisionMutation invalidates sales orders and work items", async () => {
        mockedApi.submitSalesChangeReviewDecision.mockResolvedValue({
            id: "co-1",
        } as never)
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, "invalidateQueries")

        const { result } = renderHookWithProviders(
            () => useSalesChangeReviewDecisionMutation(),
            { queryClient: client },
        )

        const input = {
            salesChangeOrderId: "co-1",
            handlerKey: "sales_change_impact_review" as const,
            decision: "APPROVE" as const,
            workItemId: "wi-1",
            expectedTaskVersion: "3",
            expectedSubjectVersion: "sv-1",
            idempotencyKey: "key-5",
        }
        await act(async () => {
            await result.current.mutateAsync(input)
        })

        expect(mockedApi.submitSalesChangeReviewDecision).toHaveBeenCalledWith(
            input,
            expect.anything(),
        )
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: salesOrderKeys.all,
        })
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: ["work-items"],
        })
    })

    it("useSubmitCardSalesApprovalDecisionMutation invalidates all sales orders", async () => {
        mockedApi.submitCardSalesApprovalDecision.mockResolvedValue({
            approval_instance_status: "APPROVED",
            work_item_id: "wi-1",
            work_item_status: "COMPLETED",
            business_result: {
                outcome: "MANAGER_APPROVED",
                sales_order_id: "so-1",
                sales_order_review_id: "sr-1",
                workflow_action_id: "wf-1",
                sales_order_commercial_status: "PENDING_OPERATIONS",
            },
        } as never)
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, "invalidateQueries")

        const { result } = renderHookWithProviders(
            () => useSubmitCardSalesApprovalDecisionMutation(),
            { queryClient: client },
        )

        const input = {
            approvalInstanceId: "ai-1",
            expectedInstanceVersion: "1",
            approvalStepInstanceId: "as-1",
            expectedStepVersion: "1",
            workItemId: "wi-1",
            expectedTaskVersion: "3",
            expectedSubjectVersion: "sv-1",
            decision: {
                salesOrderId: "so-1",
                salesOrderSubmissionId: "sub-1",
                expectedSalesOrderLockVersion: 3,
                expectedSubmissionNo: 2,
                workItemType: "CARD_SALES_MANAGER_APPROVAL",
                expectedReviewStatus: "PENDING_SALES_LEAD",
                reviewDecision: "APPROVE",
            } as const,
            idempotencyKey: "key-6",
        }
        await act(async () => {
            await result.current.mutateAsync(input)
        })

        expect(mockedApi.submitCardSalesApprovalDecision).toHaveBeenCalledWith(
            input,
            expect.anything(),
        )
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: salesOrderKeys.all,
        })
    })

    it("useCancelCardSalesApprovalMutation invalidates all sales orders", async () => {
        mockedApi.cancelCardSalesApproval.mockResolvedValue({
            approval_instance_status: "CANCELLED",
            business_result: {
                outcome: "CANCELLED_TO_EDITABLE_DRAFT",
                sales_order_id: "so-1",
                sales_order_version: "3",
                sales_order_commercial_status: "DRAFT",
                sales_order_review_status: "NOT_SUBMITTED",
                sales_order_submission_id: "sub-1",
                submission_version: "2",
                submission_status: "SUPERSEDED",
                workflow_action_id: "wf-1",
            },
        } as never)
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, "invalidateQueries")

        const { result } = renderHookWithProviders(
            () => useCancelCardSalesApprovalMutation(),
            { queryClient: client },
        )

        const input = {
            approvalInstanceId: "ai-1",
            currentStepInstanceId: "as-1",
            workItemId: "wi-1",
            expectedInstanceVersion: "1",
            expectedStepVersion: "1",
            expectedTaskVersion: "3",
            expectedSubjectVersion: "sv-1",
            reason: "申请人撤回并继续修改",
            idempotencyKey: "key-7",
        }
        await act(async () => {
            await result.current.mutateAsync(input)
        })

        expect(mockedApi.cancelCardSalesApproval).toHaveBeenCalledWith(
            input,
            expect.anything(),
        )
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: salesOrderKeys.all,
        })
    })

    it("useCreateSalesOrderExportJobMutation runs the export api without invalidating", async () => {
        const jobResult = {
            jobId: "job-1",
            status: "queued" as const,
            rowCount: 12,
            permissionVersion: "pv-w05-1",
            createdAt: "2026-08-14T00:00:00.000Z",
            downloadLabel: "销售单导出_EXP-1",
        }
        mockedApi.createSalesOrderExportJob.mockResolvedValue(jobResult)

        const { result } = renderHookWithProviders(() =>
            useCreateSalesOrderExportJobMutation(),
        )

        await act(async () => {
            const data = await result.current.mutateAsync({ rowCount: 12 })
            expect(data).toEqual(jobResult)
        })
        expect(mockedApi.createSalesOrderExportJob).toHaveBeenCalledWith(
            { rowCount: 12 },
            expect.anything(),
        )
    })
})
