import { describe, it, expect, vi, beforeEach } from "vitest"
import { act, waitFor } from "@testing-library/react"

import * as settlementsApi from "@/features/supplier-settlements/api/settlements"
import { useSettlementCenterActions } from "./use-settlement-center-actions"
import type { FormalOutcome } from "@/features/supplier-settlements/types"
import type { SettlementDetailView } from "@/features/supplier-settlements/types"
import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"

const mocks = vi.hoisted(() => ({
    profileUserid: "u1" as string | undefined,
}))

vi.mock("@/features/auth/queries", () => ({
    useAccountProfileQuery: () => ({
        data: mocks.profileUserid ? { userid: mocks.profileUserid } : undefined,
    }),
}))

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

const URL_STATE: SettlementsUrlState = {
    view: "pending",
    page: 1,
    section: "overview",
}

function makeDetail(
    overrides: Partial<SettlementDetailView> = {},
): SettlementDetailView {
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
            preparedBy: { userId: "u2", displayName: "经办人" },
            lockVersion: 3,
            subjectHash: "subj-1",
            sourceAsOf: "2026-08-14T10:00:00.000Z",
            sourceSnapshotAt: "2026-08-14T09:00:00.000Z",
            sourceSnapshotHash: "snap-1",
        },
        totals: {
            orderAmountGross: "90.00",
            freightGross: "5.00",
            serviceFeeGross: "5.00",
            refundGross: "0.00",
            erpAmountGross: "100.00",
            supplierAmountGross: "95.00",
            differenceAmountGross: "5.00",
            differenceDirectionLabel: "账单少计",
            taxBasisLabel: "含税",
            pendingCostDeltaGross: "5.00",
        },
        items: [],
        differences: [
            {
                differenceId: "d-1",
                type: "AMOUNT",
                typeLabel: "金额差异",
                status: "PENDING",
                statusLabel: "待处理",
                statusTone: "warning",
                blocking: false,
                erpSideLabel: "ERP 金额",
                erpSideAmount: "100.00",
                supplierSideLabel: "账单金额",
                supplierSideAmount: "95.00",
                amountDirectionLabel: "账单少计",
                amountGross: "5.00",
                version: 2,
                evidence: [
                    {
                        evidenceId: "e-1",
                        referenceIds: ["ticket://T-1"],
                        kind: "TICKET",
                        label: "工单",
                        by: { userId: "u1", displayName: "采购甲" },
                        at: "2026-08-14T08:00:00.000Z",
                    },
                ],
                requiresProcurementEvidence: false,
                leftFields: [],
            },
        ],
        differenceSummary: { total: 1, open: 1, blocking: 0, resolved: 0 },
        reviewRecords: [],
        workItem: {
            workItemId: "wi-1",
            taskVersion: "t1",
            workItemType: "SUPPLIER_SETTLEMENT_REVIEW",
            businessObjectType: "SUPPLIER_SETTLEMENT_STATEMENT",
            businessObjectId: "st-1",
            subjectVersion: "s1",
            processingState: "READY",
            ownerUser: { id: "u1", displayName: "复核人" },
            status: "OPEN",
            actionBlockers: [],
        },
        reviewSubmissionPolicy: {
            refreshCutoffPolicyId: "pol-1",
            version: "p1",
        },
        auditEvents: [],
        allowedActions: ["REFRESH_TRIAL", "SUBMIT_REVIEW", "CONFIRM", "REJECT"],
        actionBlockers: [],
        freshness: {
            immutableFactsAsOf: "2026-08-14T10:00:00.000Z",
            queriedAt: "2026-08-14T10:00:00.000Z",
        },
        canEditBillOrOrder: false,
        ...overrides,
    }
}

function succeededOutcome(
    overrides: Partial<FormalOutcome> = {},
): FormalOutcome {
    return {
        status: "succeeded",
        title: "已完成",
        message: "处理结果已记录",
        reference: "ref-1",
        ...overrides,
    }
}

async function renderActions(detail = makeDetail(), patchUrl = vi.fn()) {
    mockedApi.fetchSettlementDetail.mockResolvedValue(detail)
    const { result } = renderHookWithProviders(
        () =>
            useSettlementCenterActions({
                statementId: "st-1",
                workItemId: "wi-1",
                urlState: URL_STATE,
                patchUrl,
            }),
        { queryClient: createFreshQueryClient() },
    )
    await waitFor(() => expect(result.current.detailQuery.isSuccess).toBe(true))
    return { result, patchUrl }
}

beforeEach(() => {
    vi.clearAllMocks()
    mocks.profileUserid = "u1"
})

describe("useSettlementCenterActions", () => {
    it("loads the detail and derives the responsibility status", async () => {
        const { result } = await renderActions()

        expect(mockedApi.fetchSettlementDetail).toHaveBeenCalledWith({
            statementId: "st-1",
            workItemId: "wi-1",
        })
        expect(result.current.responsibilityStatus).toBe("assigned_to_me")
        expect(result.current.submitBlocker).toBeUndefined()
        expect(result.current.activeDiff?.differenceId).toBe("d-1")
    })

    it("reports assigned_to_other / missing owner / completed correctly", async () => {
        mocks.profileUserid = "u9"
        const other = await renderActions()
        expect(other.result.current.responsibilityStatus).toBe(
            "assigned_to_other",
        )

        const missingOwner = await renderActions(
            makeDetail({
                workItem: {
                    ...makeDetail().workItem!,
                    ownerUser: undefined,
                },
            }),
        )
        expect(missingOwner.result.current.responsibilityStatus).toBe(
            "assigned_to_other",
        )

        const completed = await renderActions(
            makeDetail({
                workItem: {
                    ...makeDetail().workItem!,
                    status: "COMPLETED",
                },
            }),
        )
        expect(completed.result.current.responsibilityStatus).toBe("completed")
    })

    it("blocks refresh when the source snapshot hash is missing", async () => {
        const { result } = await renderActions(
            makeDetail({
                statement: {
                    ...makeDetail().statement,
                    sourceSnapshotHash: undefined,
                },
            }),
        )

        await act(async () => {
            await result.current.onRefresh()
        })

        expect(result.current.result).toMatchObject({
            status: "blocked",
            title: "刷新试算暂不可用",
        })
        expect(mockedApi.refreshSettlementTrial).not.toHaveBeenCalled()
    })

    it("refreshes the trial with the expected lock version and hash", async () => {
        mockedApi.refreshSettlementTrial.mockResolvedValue(succeededOutcome())
        const { result } = await renderActions()

        await act(async () => {
            await result.current.onRefresh()
        })

        expect(mockedApi.refreshSettlementTrial).toHaveBeenCalledWith(
            expect.objectContaining({
                statementId: "st-1",
                expectedLockVersion: 3,
                expectedSourceSnapshotHash: "snap-1",
                requestId: expect.any(String),
                idempotencyKey: expect.any(String),
            }),
            expect.anything(),
        )
        expect(result.current.result?.status).toBe("succeeded")
    })

    it("resolves the active difference and closes the dialog on success", async () => {
        mockedApi.resolveDifference.mockResolvedValue(succeededOutcome())
        const { result } = await renderActions()
        act(() => {
            result.current.setResolveOpen(true)
        })

        await act(async () => {
            await result.current.onResolve()
        })

        expect(mockedApi.resolveDifference).toHaveBeenCalledWith(
            expect.objectContaining({
                statementId: "st-1",
                differenceId: "d-1",
                expectedLockVersion: 3,
                expectedDifferenceVersion: 2,
                resolution: "ERP_ACCEPTED",
                reasonCode: "ACCEPT_BILL",
                evidenceReferenceIds: ["ticket://T-1"],
                operationId: expect.stringMatching(/^w27:resolve-difference:/),
                idempotencyKey: expect.stringMatching(
                    /^w27:resolve-difference:/,
                ),
            }),
            expect.anything(),
        )
        expect(result.current.result?.status).toBe("succeeded")
        expect(result.current.resolveOpen).toBe(false)
    })

    it("reuses the same command identity for an unresolved outcome", async () => {
        mockedApi.resolveDifference.mockResolvedValue({
            status: "unknown",
            title: "处理结果待确认",
            message: "请稍后查询",
        })
        const { result } = await renderActions()
        act(() => {
            result.current.setResolveOpen(true)
        })

        await act(async () => {
            await result.current.onResolve()
        })
        await act(async () => {
            await result.current.onResolve()
        })

        const calls = mockedApi.resolveDifference.mock.calls
        expect(calls[0][0].operationId).toBe(calls[1][0].operationId)
        expect(calls[0][0].idempotencyKey).toBe(calls[1][0].idempotencyKey)
        expect(result.current.resolveOpen).toBe(true)
    })

    it("reports a rejected result when resolving fails", async () => {
        mockedApi.resolveDifference.mockRejectedValue(new Error("boom"))
        const { result } = await renderActions()

        await act(async () => {
            await result.current.onResolve()
        })

        expect(result.current.result).toMatchObject({
            status: "rejected",
            title: "结论登记未完成",
        })
    })

    it("blocks evidence without a reference id", async () => {
        const { result } = await renderActions()

        await act(async () => {
            await result.current.onEvidence()
        })

        expect(result.current.result).toMatchObject({
            status: "blocked",
            title: "缺少正式证据引用",
        })
        expect(mockedApi.appendDifferenceEvidence).not.toHaveBeenCalled()
    })

    it("appends evidence and resets the form on success", async () => {
        mockedApi.appendDifferenceEvidence.mockResolvedValue(succeededOutcome())
        const { result } = await renderActions()
        act(() => {
            result.current.setEvidenceOpen(true)
            result.current.setEvidenceReferenceId("  ticket://T-2  ")
            result.current.setEvidenceComment("补充说明")
        })

        await act(async () => {
            await result.current.onEvidence()
        })

        expect(mockedApi.appendDifferenceEvidence).toHaveBeenCalledWith(
            expect.objectContaining({
                statementId: "st-1",
                differenceId: "d-1",
                expectedDifferenceVersion: 2,
                evidenceReferenceIds: ["ticket://T-2"],
                opinionCode: "PROCUREMENT_NOTE",
                comment: "补充说明",
            }),
            expect.anything(),
        )
        expect(result.current.result?.status).toBe("succeeded")
        expect(result.current.evidenceOpen).toBe(false)
        expect(result.current.evidenceComment).toBe("")
        expect(result.current.evidenceReferenceId).toBe("")
    })

    it("blocks submit review when subject hash or policy is missing", async () => {
        const { result } = await renderActions(
            makeDetail({
                statement: {
                    ...makeDetail().statement,
                    subjectHash: undefined,
                },
            }),
        )

        await act(async () => {
            await result.current.onSubmitReview()
        })

        expect(result.current.result).toMatchObject({
            status: "blocked",
            title: "提交复核暂不可用",
        })
        expect(mockedApi.submitSettlementReview).not.toHaveBeenCalled()
    })

    it("submits review and navigates to the review section on success", async () => {
        mockedApi.submitSettlementReview.mockResolvedValue(succeededOutcome())
        const { result, patchUrl } = await renderActions()
        act(() => {
            result.current.setSubmitOpen(true)
            result.current.setReviewerUserId("reviewer-1")
        })

        await act(async () => {
            await result.current.onSubmitReview()
        })

        expect(mockedApi.submitSettlementReview).toHaveBeenCalledWith(
            expect.objectContaining({
                statementId: "st-1",
                expectedLockVersion: 3,
                subjectHash: "subj-1",
                refreshCutoffPolicyId: "pol-1",
                expectedRefreshCutoffPolicyVersion: "p1",
                reviewerUserId: "reviewer-1",
            }),
            expect.anything(),
        )
        expect(patchUrl).toHaveBeenCalledWith({ section: "review" })
        expect(result.current.submitOpen).toBe(false)
    })

    it("blocks confirm without a review work item", async () => {
        const { result } = await renderActions(
            makeDetail({ workItem: undefined }),
        )

        await act(async () => {
            await result.current.onConfirm()
        })

        expect(result.current.result).toMatchObject({
            status: "blocked",
            title: "无复核任务",
        })
        expect(mockedApi.decideSettlementReview).not.toHaveBeenCalled()
    })

    it("does nothing on confirm when the task is not assigned to the user", async () => {
        mocks.profileUserid = "u9"
        const { result } = await renderActions()

        await act(async () => {
            await result.current.onConfirm()
        })

        expect(mockedApi.decideSettlementReview).not.toHaveBeenCalled()
        expect(result.current.result).toBeNull()
    })

    it("confirms the settlement and navigates to payable on success", async () => {
        mockedApi.decideSettlementReview.mockResolvedValue(succeededOutcome())
        const { result, patchUrl } = await renderActions()
        act(() => {
            result.current.setConfirmOpen(true)
        })

        await act(async () => {
            await result.current.onConfirm()
        })

        expect(mockedApi.decideSettlementReview).toHaveBeenCalledWith(
            expect.objectContaining({
                statementId: "st-1",
                workItemId: "wi-1",
                expectedTaskVersion: "t1",
                expectedSubjectVersion: "s1",
                expectedLockVersion: 3,
                action: "CONFIRM",
            }),
            expect.anything(),
        )
        expect(patchUrl).toHaveBeenCalledWith({ section: "payable" })
        expect(result.current.confirmOpen).toBe(false)
    })

    it("rejects the review with the selected reason code and closes on rejection", async () => {
        mockedApi.decideSettlementReview.mockResolvedValue({
            status: "rejected",
            title: "已驳回",
            message: "退回经办",
        })
        const { result } = await renderActions()
        act(() => {
            result.current.setRejectOpen(true)
            result.current.setRejectReason("AMOUNT_MISMATCH")
        })

        await act(async () => {
            await result.current.onReject()
        })

        expect(mockedApi.decideSettlementReview).toHaveBeenCalledWith(
            expect.objectContaining({
                action: "REJECT",
                reasonCode: "AMOUNT_MISMATCH",
            }),
            expect.anything(),
        )
        expect(result.current.rejectOpen).toBe(false)

        // 未选原因码时回退默认值
        act(() => {
            result.current.setRejectOpen(true)
            result.current.setRejectReason("")
        })
        await act(async () => {
            await result.current.onReject()
        })
        const lastCall = mockedApi.decideSettlementReview.mock.calls.at(-1)
        expect(lastCall?.[0].reasonCode).toBe("NEEDS_MORE_EVIDENCE")
    })
})
