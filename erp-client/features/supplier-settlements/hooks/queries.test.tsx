import { describe, it, expect, vi, beforeEach } from "vitest"
import { act, waitFor } from "@testing-library/react"

import * as settlementsApi from "@/features/supplier-settlements/api/settlements"
import {
    useAppendEvidenceMutation,
    useCreateDraftMutation,
    useRefreshTrialMutation,
    useResolveDifferenceMutation,
    useReviewDecisionMutation,
    useSettlementDetailQuery,
    useSettlementListQuery,
    useSubmitReviewMutation,
} from "./queries"
import type { ListQueryInput } from "@/features/supplier-settlements/api/settlements"
import type {
    AppendEvidenceInput,
    CreateDraftInput,
    FormalOutcome,
    RefreshDraftInput,
    ResolveDifferenceInput,
    ReviewDecisionInput,
    SettlementDetailView,
    SettlementListView,
    SubmitReviewInput,
} from "@/features/supplier-settlements/types"
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"

vi.mock("@/features/supplier-settlements/api/settlements", () => ({
    appendDifferenceEvidence: vi.fn(),
    createSettlementDraft: vi.fn(),
    decideSettlementReview: vi.fn(),
    fetchSettlementDetail: vi.fn(),
    fetchSettlementList: vi.fn(),
    refreshSettlementTrial: vi.fn(),
    resolveDifference: vi.fn(),
    submitSettlementReview: vi.fn(),
}))

const mockedApi = vi.mocked(settlementsApi)

const LIST_INPUT: ListQueryInput = { view: "pending", page: 1 }

const LIST_VIEW: SettlementListView = {
    view: "pending",
    rows: [],
    page: 1,
    pageSize: 50,
    total: 0,
    totals: {
        pendingReconcile: 0,
        hasDifference: 0,
        pendingReview: 0,
        confirmedAmountThisPeriod: "0.00",
    },
    metrics: {
        pending: 0,
        hasDifference: 0,
        pendingReview: 0,
        confirmedAmount: "0.00",
    },
    suppliers: [],
    emptyReason: "NO_STATEMENTS",
    hasModulePermission: true,
    hasDataScope: true,
    permissionVersion: "server",
    sourceAsOf: "2026-08-14T10:00:00.000Z",
    queriedAt: "2026-08-14T10:00:00.000Z",
    filterSummary: "默认待处理视图",
}

function makeDetail(): SettlementDetailView {
    return {
        statement: {
            id: "st-1",
            statementNo: "JS-2026-001",
            supplierId: "sup-1",
            supplierName: "示例供应商",
            periodStart: "2026-08-01",
            periodEnd: "2026-08-31",
            periodLabel: "2026年8月",
            erpAmountGross: "100.00",
            status: "DRAFT",
            statusLabel: "草稿",
            statusTone: "neutral",
            lockVersion: 3,
            sourceAsOf: "2026-08-14T10:00:00.000Z",
            sourceSnapshotAt: "2026-08-14T09:00:00.000Z",
        },
        totals: {
            orderAmountGross: "100.00",
            freightGross: "0.00",
            serviceFeeGross: "0.00",
            refundGross: "0.00",
            erpAmountGross: "100.00",
            taxBasisLabel: "含税",
        },
        items: [],
        differences: [],
        differenceSummary: { total: 0, open: 0, blocking: 0, resolved: 0 },
        reviewRecords: [],
        auditEvents: [],
        allowedActions: [],
        actionBlockers: [],
        freshness: {
            immutableFactsAsOf: "2026-08-14T10:00:00.000Z",
            queriedAt: "2026-08-14T10:00:00.000Z",
        },
        canEditBillOrOrder: false,
    }
}

function makeOutcome(
    status: FormalOutcome["status"],
): FormalOutcome {
    return { status, title: "标题", message: "说明" }
}

const CREATE_INPUT: CreateDraftInput = {
    supplierId: "sup-1",
    periodStart: "2026-08-01",
    periodEnd: "2026-08-31",
    requestId: "req-1",
    idempotencyKey: "idem-1",
}

const REFRESH_INPUT: RefreshDraftInput = {
    statementId: "st-1",
    expectedLockVersion: 3,
    expectedSourceSnapshotHash: "snap-1",
    requestId: "req-1",
    idempotencyKey: "idem-1",
}

const EVIDENCE_INPUT: AppendEvidenceInput = {
    statementId: "st-1",
    differenceId: "d-1",
    expectedDifferenceVersion: 2,
    evidenceReferenceIds: ["ticket://T-1"],
    requestId: "req-1",
    idempotencyKey: "idem-1",
}

const RESOLVE_INPUT: ResolveDifferenceInput = {
    statementId: "st-1",
    differenceId: "d-1",
    expectedLockVersion: 3,
    expectedDifferenceVersion: 2,
    resolution: "ERP_ACCEPTED",
    reasonCode: "ACCEPT_BILL",
    evidenceReferenceIds: [],
    operationId: "op-1",
    idempotencyKey: "idem-1",
}

const SUBMIT_INPUT: SubmitReviewInput = {
    statementId: "st-1",
    expectedLockVersion: 3,
    subjectHash: "subj-1",
    refreshCutoffPolicyId: "pol-1",
    expectedRefreshCutoffPolicyVersion: "p1",
    operationId: "op-1",
    idempotencyKey: "idem-1",
}

const DECISION_INPUT: ReviewDecisionInput = {
    statementId: "st-1",
    workItemId: "wi-1",
    expectedTaskVersion: "t1",
    expectedSubjectVersion: "s1",
    expectedLockVersion: 3,
    action: "CONFIRM",
    operationId: "op-1",
    idempotencyKey: "idem-1",
}

async function mutateAndWait<Input>(
    mutateAsync: (input: Input) => Promise<unknown>,
    input: Input,
) {
    let outcome: unknown
    await act(async () => {
        outcome = await mutateAsync(input)
    })
    return outcome
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("useSettlementListQuery", () => {
    it("passes the input to the queryFn under a stable key", async () => {
        mockedApi.fetchSettlementList.mockResolvedValue(LIST_VIEW)

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useSettlementListQuery(LIST_INPUT),
            { queryClient: client },
        )

        await waitFor(() =>
            expect(result.current.data).toEqual(LIST_VIEW),
        )
        expect(mockedApi.fetchSettlementList).toHaveBeenCalledWith(LIST_INPUT)
        expect(client.getQueryCache().getAll().map((q) => q.queryKey)).toEqual([
            ["supplier-settlements", "list", LIST_INPUT],
        ])
    })

    it("exposes the error state on failure", async () => {
        mockedApi.fetchSettlementList.mockRejectedValue(new Error("boom"))

        const { result } = renderHookWithProviders(
            () => useSettlementListQuery(LIST_INPUT),
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe("useSettlementDetailQuery", () => {
    it("is disabled without a statement id", async () => {
        mockedApi.fetchSettlementDetail.mockResolvedValue(makeDetail())

        const { result } = renderHookWithProviders(() =>
            useSettlementDetailQuery(undefined, "wi-1"),
        )

        await waitFor(() => expect(result.current.isPending).toBe(true))
        expect(mockedApi.fetchSettlementDetail).not.toHaveBeenCalled()
    })

    it("fetches the detail and keys the cache with the work item id", async () => {
        mockedApi.fetchSettlementDetail.mockResolvedValue(makeDetail())

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useSettlementDetailQuery("st-1", "wi-1"),
            { queryClient: client },
        )

        await waitFor(() => expect(result.current.data?.statement.id).toBe("st-1"))
        expect(mockedApi.fetchSettlementDetail).toHaveBeenCalledWith({
            statementId: "st-1",
            workItemId: "wi-1",
        })
        expect(client.getQueryCache().getAll().map((q) => q.queryKey)).toEqual([
            ["supplier-settlements", "detail", "st-1", "wi-1"],
        ])
    })

    it("uses null in the key when no work item id is given", async () => {
        mockedApi.fetchSettlementDetail.mockResolvedValue(makeDetail())

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useSettlementDetailQuery("st-1"),
            { queryClient: client },
        )

        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(client.getQueryCache().getAll().map((q) => q.queryKey)).toEqual([
            ["supplier-settlements", "detail", "st-1", null],
        ])
    })
})

describe("settlement mutations", () => {
    it("createDraft wires the mutationFn and invalidates on success", async () => {
        mockedApi.createSettlementDraft.mockResolvedValue(
            makeOutcome("succeeded"),
        )
        const client = createFreshQueryClient()
        client.setQueryData(["supplier-settlements", "list", LIST_INPUT], LIST_VIEW)
        const { result } = renderHookWithProviders(
            () => useCreateDraftMutation(),
            { queryClient: client },
        )

        const outcome = await mutateAndWait(
            result.current.mutateAsync,
            CREATE_INPUT,
        )

        expect(mockedApi.createSettlementDraft).toHaveBeenCalledWith(
            CREATE_INPUT,
            expect.anything(),
        )
        expect(outcome).toEqual(makeOutcome("succeeded"))
        await waitFor(() =>
            expect(
                client.getQueryState([
                    "supplier-settlements",
                    "list",
                    LIST_INPUT,
                ])?.isInvalidated,
            ).toBe(true),
        )
    })

    it("createDraft does not invalidate on failure", async () => {
        mockedApi.createSettlementDraft.mockResolvedValue(makeOutcome("failed"))
        const client = createFreshQueryClient()
        client.setQueryData(["supplier-settlements", "list", LIST_INPUT], LIST_VIEW)
        const { result } = renderHookWithProviders(
            () => useCreateDraftMutation(),
            { queryClient: client },
        )

        await mutateAndWait(result.current.mutateAsync, CREATE_INPUT)

        const state = client.getQueryState([
            "supplier-settlements",
            "list",
            LIST_INPUT,
        ])
        expect(state?.isInvalidated).toBe(false)
    })

    it("refreshTrial wires the mutationFn", async () => {
        mockedApi.refreshSettlementTrial.mockResolvedValue(
            makeOutcome("succeeded"),
        )
        const { result } = renderHookWithProviders(() =>
            useRefreshTrialMutation(),
        )

        await mutateAndWait(result.current.mutateAsync, REFRESH_INPUT)

        expect(mockedApi.refreshSettlementTrial).toHaveBeenCalledWith(
            REFRESH_INPUT,
            expect.anything(),
        )
    })

    it("appendEvidence wires the mutationFn", async () => {
        mockedApi.appendDifferenceEvidence.mockResolvedValue(
            makeOutcome("succeeded"),
        )
        const { result } = renderHookWithProviders(() =>
            useAppendEvidenceMutation(),
        )

        await mutateAndWait(result.current.mutateAsync, EVIDENCE_INPUT)

        expect(mockedApi.appendDifferenceEvidence).toHaveBeenCalledWith(
            EVIDENCE_INPUT,
            expect.anything(),
        )
    })

    it("resolveDifference wires the mutationFn", async () => {
        mockedApi.resolveDifference.mockResolvedValue(makeOutcome("succeeded"))
        const { result } = renderHookWithProviders(() =>
            useResolveDifferenceMutation(),
        )

        await mutateAndWait(result.current.mutateAsync, RESOLVE_INPUT)

        expect(mockedApi.resolveDifference).toHaveBeenCalledWith(
            RESOLVE_INPUT,
            expect.anything(),
        )
    })

    it("submitReview wires the mutationFn", async () => {
        mockedApi.submitSettlementReview.mockResolvedValue(
            makeOutcome("succeeded"),
        )
        const { result } = renderHookWithProviders(() =>
            useSubmitReviewMutation(),
        )

        await mutateAndWait(result.current.mutateAsync, SUBMIT_INPUT)

        expect(mockedApi.submitSettlementReview).toHaveBeenCalledWith(
            SUBMIT_INPUT,
            expect.anything(),
        )
    })

    it("reviewDecision invalidates on terminal outcomes but not on blocked", async () => {
        mockedApi.decideSettlementReview.mockResolvedValue(
            makeOutcome("succeeded"),
        )
        const client = createFreshQueryClient()
        client.setQueryData(["supplier-settlements", "list", LIST_INPUT], LIST_VIEW)
        const { result } = renderHookWithProviders(
            () => useReviewDecisionMutation(),
            { queryClient: client },
        )

        await mutateAndWait(result.current.mutateAsync, DECISION_INPUT)
        expect(mockedApi.decideSettlementReview).toHaveBeenCalledWith(
            DECISION_INPUT,
            expect.anything(),
        )
        await waitFor(() =>
            expect(
                client.getQueryState([
                    "supplier-settlements",
                    "list",
                    LIST_INPUT,
                ])?.isInvalidated,
            ).toBe(true),
        )

        client.getQueryState(["supplier-settlements", "list", LIST_INPUT])
        mockedApi.decideSettlementReview.mockResolvedValue(
            makeOutcome("blocked"),
        )
        const client2 = createFreshQueryClient()
        client2.setQueryData(
            ["supplier-settlements", "list", LIST_INPUT],
            LIST_VIEW,
        )
        const second = renderHookWithProviders(
            () => useReviewDecisionMutation(),
            { queryClient: client2 },
        )
        await mutateAndWait(second.result.current.mutateAsync, DECISION_INPUT)
        expect(
            client2.getQueryState(["supplier-settlements", "list", LIST_INPUT])
                ?.isInvalidated,
        ).toBe(false)
    })
})
