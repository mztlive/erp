import { act } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { ReadonlyURLSearchParams } from "next/navigation"

const formMocks = vi.hoisted(() => {
    type FakeFormOpts = {
        defaultValues?: Record<string, unknown>
        onSubmit?: (arg: {
            value: Record<string, unknown>
        }) => Promise<void> | void
    }
    const makeFakeForm = (opts: FakeFormOpts = {}) => {
        const values: Record<string, unknown> = {
            ...opts.defaultValues,
        }
        return {
            AppField: () => null,
            AppForm: ({ children }: { children?: unknown }) => children,
            getFieldValue: (name: string) => values[name],
            setFieldValue: (name: string, value: unknown) => {
                values[name] = value
            },
            handleSubmit: async () => {
                await opts.onSubmit?.({ value: { ...values } })
            },
        }
    }
    return {
        useAppForm: vi.fn((opts?: FakeFormOpts) => makeFakeForm(opts)),
    }
})

vi.mock("@/components/form", () => ({
    useAppForm: formMocks.useAppForm,
}))

vi.mock("@/features/mall-sync/hooks/queries", () => ({
    useMallSyncPageQuery: vi.fn(),
    useConfirmMappingMutation: vi.fn(),
    useReapplyMutation: vi.fn(),
    useResolveUnknownReapplyMutation: vi.fn(),
    useRetryJobMutation: vi.fn(),
    useRequestSourceFixMutation: vi.fn(),
    useTriggerIncrementalMutation: vi.fn(),
    useTriggerSingleOrderMutation: vi.fn(),
}))

vi.mock("@/features/auth/queries", () => ({
    useAccountProfileQuery: vi.fn(),
}))

vi.mock("@/features/work-items", () => ({
    useWorkItemResponsibilityMutation: vi.fn(),
}))

import * as mallSyncQueries from "@/features/mall-sync/hooks/queries"
import * as authQueries from "@/features/auth/queries"
import * as workItems from "@/features/work-items"
import { renderHookWithProviders } from "@/features/test-utils"
import type {
    MallSyncJobRow,
    MallSyncPageView,
    MappingTaskView,
} from "@/features/mall-sync/types"
import { useMallSyncPage } from "./use-mall-sync-page"

const apiMocks = {
    refetch: vi.fn(),
    responsibility: vi.fn(),
    confirm: vi.fn(),
    sourceFix: vi.fn(),
    reapply: vi.fn(),
    resolveReapply: vi.fn(),
    retryJob: vi.fn(),
    triggerInc: vi.fn(),
    triggerSo: vi.fn(),
}

beforeEach(() => {
    vi.clearAllMocks()
    apiMocks.refetch.mockResolvedValue(undefined)
    vi.mocked(authQueries.useAccountProfileQuery).mockReturnValue({
        data: { userid: "user-1" },
    } as never)
    vi.mocked(mallSyncQueries.useMallSyncPageQuery).mockReturnValue({
        data: undefined,
        isPending: true,
        isError: false,
        error: null,
        refetch: apiMocks.refetch,
    } as never)
    vi.mocked(workItems.useWorkItemResponsibilityMutation).mockReturnValue({
        mutateAsync: apiMocks.responsibility,
        isPending: false,
    } as never)
    vi.mocked(mallSyncQueries.useConfirmMappingMutation).mockReturnValue({
        mutateAsync: apiMocks.confirm,
        isPending: false,
    } as never)
    vi.mocked(mallSyncQueries.useRequestSourceFixMutation).mockReturnValue({
        mutateAsync: apiMocks.sourceFix,
        isPending: false,
    } as never)
    vi.mocked(mallSyncQueries.useReapplyMutation).mockReturnValue({
        mutateAsync: apiMocks.reapply,
        isPending: false,
    } as never)
    vi.mocked(mallSyncQueries.useResolveUnknownReapplyMutation).mockReturnValue(
        {
            mutateAsync: apiMocks.resolveReapply,
            isPending: false,
        } as never,
    )
    vi.mocked(mallSyncQueries.useRetryJobMutation).mockReturnValue({
        mutateAsync: apiMocks.retryJob,
        isPending: false,
    } as never)
    vi.mocked(mallSyncQueries.useTriggerIncrementalMutation).mockReturnValue({
        mutateAsync: apiMocks.triggerInc,
        isPending: false,
    } as never)
    vi.mocked(mallSyncQueries.useTriggerSingleOrderMutation).mockReturnValue({
        mutateAsync: apiMocks.triggerSo,
        isPending: false,
    } as never)
})

function makePageView(
    overrides: Partial<MallSyncPageView> = {},
): MallSyncPageView {
    return {
        context: {
            sourceSystem: {
                id: "src-1",
                code: "mall",
                name: "示例商城",
                environmentLabel: "生产",
            },
            ownership: {
                businessType: "VOUCHER",
                stage: "FIRST_PHASE_MALL_OWNED",
                originSystemSummary: "MALL",
                syncDirection: "MALL_TO_ERP_COMMERCIAL_FACT",
                firstPhasePollingEnabled: true,
                mallWriteBoundary: "商城可写",
                erpWriteBoundary: "ERP 可写",
            },
            freshness: { viewProjectedAt: "2026-08-14T00:00:00Z" },
            metrics: [],
            hasSourceScope: true,
            scheduledIncrementalNote: "",
        },
        jobs: [],
        snapshots: [],
        mappingTasks: [],
        reconciliation: null,
        history: [],
        ...overrides,
    }
}

function makeMappingTask(
    overrides: Partial<MappingTaskView> = {},
): MappingTaskView {
    return {
        mappingTaskId: "mt-1",
        sourceSnapshotId: "ss-1",
        externalOrderNo: "SO-100",
        mappingType: "CUSTOMER",
        mappingTypeLabel: "客户映射",
        mappingTaskStatus: "PENDING",
        mappingTaskStatusLabel: "待处理",
        sourceEvidence: [],
        candidateTargets: [
            {
                objectType: "CUSTOMER",
                objectId: "obj-1",
                stableNo: "C-001",
                label: "客户一",
                currentRevisionId: "r1",
                eligibility: "ELIGIBLE",
                reason: "",
            },
        ],
        currentTargets: [],
        impactSummary: "",
        resolutionHistory: [],
        allowedActions: ["CONFIRM_TARGET", "REQUEST_SOURCE_FIX"],
        actionBlockers: [],
        lockVersion: 3,
        ownerRoutingState: "CONFIGURED",
        ownerRole: "OPERATIONS",
        ownerRoleLabel: "运营",
        ownerUserId: "user-1",
        workItem: {
            workItemId: "wi-1",
            workItemType: "BUSINESS_EXCEPTION",
            businessObjectType: "MASTER_MAPPING_TASK",
            businessObjectId: "mt-1",
            subjectVersion: "v1",
            taskVersion: "5",
            status: "OPEN",
            statusLabel: "待处理",
            processingState: "READY",
            ownerUser: { id: "user-1", displayName: "张三" },
        },
        ...overrides,
    } as MappingTaskView
}

function makeJobRow(overrides: Partial<MallSyncJobRow> = {}): MallSyncJobRow {
    return {
        jobId: "job-1",
        jobNo: "J-001",
        jobType: "INCREMENTAL",
        jobTypeLabel: "增量拉取",
        status: "PARTIAL_FAILED",
        statusLabel: "部分失败",
        statusTone: "warning",
        pageCount: 2,
        itemCount: 10,
        errorCount: 3,
        startedAt: "2026-08-14T00:00:00Z",
        triggeredBy: "admin",
        watermarkAdvanced: true,
        allowedActions: [],
        actionBlockers: [],
        ...overrides,
    }
}

function configuredWorkItem(task: MappingTaskView) {
    if (task.ownerRoutingState !== "CONFIGURED" || !task.workItem) {
        throw new Error("fixture must be CONFIGURED")
    }
    return task.workItem
}

function makePageData(
    overrides: Partial<MallSyncPageView> = {},
): MallSyncPageView {
    return makePageView({
        mappingTasks: [makeMappingTask()],
        selectedMappingTask: makeMappingTask(),
        ...overrides,
    })
}

type PageInput = Parameters<typeof useMallSyncPage>[0]

function makeInput(overrides: Partial<PageInput> = {}): PageInput {
    return {
        view: "mapping",
        q: "",
        jobId: undefined,
        snapshotId: undefined,
        mappingTaskId: "mt-1",
        workItemId: undefined,
        differenceId: undefined,
        queueContextId: "queue:W17:mall-sync",
        searchParams: new URLSearchParams(
            "view=mapping&mappingTaskId=mt-1",
        ) as unknown as ReadonlyURLSearchParams,
        patchUrl: vi.fn(),
        ...overrides,
    }
}

function setPageData(data: MallSyncPageView) {
    vi.mocked(mallSyncQueries.useMallSyncPageQuery).mockReturnValue({
        data,
        isPending: false,
        isError: false,
        error: null,
        refetch: apiMocks.refetch,
    } as never)
}

function renderPage(input: PageInput = makeInput()) {
    return renderHookWithProviders(() => useMallSyncPage(input))
}

describe("useMallSyncPage", () => {
    it("forwards pending state when the page query is loading", () => {
        const { result } = renderPage()
        expect(result.current.pageQuery.isPending).toBe(true)
        expect(result.current.data).toBeUndefined()
        expect(result.current.sealed).toBe(true)
        expect(result.current.firstPhase).toBe(false)
        expect(result.current.canManualSync).toBe(false)
        expect(result.current.manualSyncDisabledReason).toBe(
            "已封存：无第一期写动作",
        )
    })

    it("derives the query input from URL state", () => {
        renderPage(
            makeInput({
                q: "abc",
                jobId: "j1",
                snapshotId: "s1",
                workItemId: "w1",
                differenceId: "d1",
            }),
        )
        expect(
            vi.mocked(mallSyncQueries.useMallSyncPageQuery),
        ).toHaveBeenCalledWith({
            view: "mapping",
            q: "abc",
            jobId: "j1",
            snapshotId: "s1",
            mappingTaskId: "mt-1",
            workItemId: "w1",
            differenceId: "d1",
            queueContextId: "queue:W17:mall-sync",
            owner: "all",
        })
    })

    it("omits an empty q from the query input", () => {
        renderPage()
        const input = vi.mocked(mallSyncQueries.useMallSyncPageQuery).mock
            .calls[0]?.[0]
        expect(input).toMatchObject({ q: undefined, owner: "all" })
    })

    it("derives stage flags from ownership", () => {
        setPageData(makePageData())
        const { result } = renderPage()
        expect(result.current.firstPhase).toBe(true)
        expect(result.current.sealed).toBe(false)
        expect(result.current.canManualSync).toBe(true)
        expect(result.current.manualSyncDisabledReason).toBeNull()
    })

    it("derives sealed state", () => {
        const data = makePageData()
        data.context.ownership.stage = "ARCHIVED"
        setPageData(data)
        const { result } = renderPage()
        expect(result.current.sealed).toBe(true)
        expect(result.current.firstPhase).toBe(false)
        expect(result.current.canManualSync).toBe(false)
        expect(result.current.manualSyncDisabledReason).toBe(
            "已封存：无第一期写动作",
        )
    })

    it("disables manual sync when the source is unavailable", () => {
        const data = makePageData()
        data.context.sourceUnavailable = true
        setPageData(data)
        const { result } = renderPage()
        expect(result.current.canManualSync).toBe(false)
        expect(result.current.manualSyncDisabledReason).toBe(
            "来源不可用时不新建推进任务（可重试既有失败）",
        )
    })

    it("computes the mapping index", () => {
        const mt2 = makeMappingTask({ mappingTaskId: "mt-2" })
        setPageData(
            makePageData({
                mappingTasks: [
                    makeMappingTask(),
                    mt2,
                    makeMappingTask({ mappingTaskId: "mt-3" }),
                ],
                selectedMappingTask: mt2,
            }),
        )
        const { result } = renderPage()
        expect(result.current.mappingIndex).toEqual({ current: 2, total: 3 })
    })

    it("computes 1/… when the selected task is missing from the list", () => {
        setPageData(
            makePageData({
                mappingTasks: [
                    makeMappingTask(),
                    makeMappingTask({ mappingTaskId: "mt-9" }),
                ],
                selectedMappingTask: makeMappingTask({
                    mappingTaskId: "mt-x",
                }),
            }),
        )
        const { result } = renderPage()
        expect(result.current.mappingIndex).toEqual({ current: 1, total: 2 })
    })

    it("computes 0/0 without mapping tasks", () => {
        setPageData(
            makePageData({
                mappingTasks: [],
                selectedMappingTask: undefined,
            }),
        )
        const { result } = renderPage()
        expect(result.current.mappingIndex).toEqual({ current: 0, total: 0 })
    })

    it("slices page jobs by pagination state", () => {
        const jobs = Array.from({ length: 25 }, (_, i) =>
            makeJobRow({ jobId: `job-${i}` }),
        )
        setPageData(makePageData({ jobs }))
        const { result } = renderPage()
        expect(result.current.pageJobs).toHaveLength(20)
        act(() => {
            result.current.setPagination({ pageIndex: 1, pageSize: 20 })
        })
        expect(result.current.pageJobs).toHaveLength(5)
        expect(result.current.pageJobs[0]?.jobId).toBe("job-20")
    })

    it("resets candidate and action error when the mapping task changes", () => {
        setPageData(makePageData())
        const { result, rerender } = renderPage()
        act(() => {
            result.current.setSelectedCandidateId("obj-1")
            result.current.setActionError("boom")
        })
        expect(result.current.selectedCandidateId).toBe("obj-1")
        setPageData(
            makePageData({
                selectedMappingTask: makeMappingTask({
                    mappingTaskId: "mt-2",
                }),
            }),
        )
        rerender()
        expect(result.current.selectedCandidateId).toBeNull()
        expect(result.current.actionError).toBeNull()
    })

    it("derives responsibility status for an owned work item", () => {
        setPageData(makePageData())
        const { result } = renderPage()
        expect(result.current.responsibilityStatus).toBe("assigned_to_me")
    })

    it("derives responsibility status variants", () => {
        const other = makeMappingTask()
        configuredWorkItem(other).ownerUser = {
            id: "user-9",
            displayName: "李四",
        }
        setPageData(makePageData({ selectedMappingTask: other }))
        expect(renderPage().result.current.responsibilityStatus).toBe(
            "assigned_to_other",
        )

        const missingOwner = makeMappingTask()
        configuredWorkItem(missingOwner).ownerUser = undefined
        setPageData(makePageData({ selectedMappingTask: missingOwner }))
        expect(renderPage().result.current.responsibilityStatus).toBe(
            "assigned_to_other",
        )

        const completed = makeMappingTask()
        configuredWorkItem(completed).status = "COMPLETED"
        setPageData(makePageData({ selectedMappingTask: completed }))
        expect(renderPage().result.current.responsibilityStatus).toBe(
            "completed",
        )

        const closed = makeMappingTask()
        configuredWorkItem(closed).status = "CLOSED"
        setPageData(makePageData({ selectedMappingTask: closed }))
        expect(renderPage().result.current.responsibilityStatus).toBe("closed")

        const blocked = makeMappingTask()
        configuredWorkItem(blocked).processingState = "APPROVAL_BLOCKED"
        setPageData(makePageData({ selectedMappingTask: blocked }))
        expect(renderPage().result.current.responsibilityStatus).toBe("blocked")

        setPageData(
            makePageData({
                selectedMappingTask: makeMappingTask({
                    ownerRoutingState: "MISSING",
                }),
            }),
        )
        expect(renderPage().result.current.responsibilityStatus).toBe("blocked")
    })

    it("allows confirming only when every condition is met", () => {
        setPageData(makePageData())
        const { result } = renderPage()
        expect(result.current.canConfirmMapping).toBe(false)
        act(() => {
            result.current.setSelectedCandidateId("obj-1")
        })
        expect(result.current.canConfirmMapping).toBe(true)

        const conflicted = makeMappingTask({ hasConflict: true })
        setPageData(makePageData({ selectedMappingTask: conflicted }))
        const second = renderPage().result
        act(() => {
            second.current.setSelectedCandidateId("obj-1")
        })
        expect(second.current.canConfirmMapping).toBe(false)
    })

    it("confirms the selected candidate and reports success", async () => {
        apiMocks.confirm.mockResolvedValue({
            status: "succeeded",
            mappingTaskId: "mt-1",
            mappingTaskStatus: "RESOLVED",
            externalIdentityMapId: "em-1",
            mappingTargetId: "t-1",
            recordedAt: "2026-08-14T00:00:00Z",
            message: "已确认",
        })
        const patchUrl = vi.fn()
        setPageData(
            makePageData({
                mappingTasks: [
                    makeMappingTask(),
                    makeMappingTask({ mappingTaskId: "mt-2" }),
                ],
                selectedMappingTask: makeMappingTask(),
            }),
        )
        const { result } = renderPage(makeInput({ patchUrl }))
        act(() => {
            result.current.setSelectedCandidateId("obj-1")
        })
        await act(async () => {
            await result.current.handleConfirm()
        })
        expect(apiMocks.confirm).toHaveBeenCalledWith(
            expect.objectContaining({
                mappingTaskId: "mt-1",
                targetObjectId: "obj-1",
                relationRole: "CUSTOMER",
                evidenceNote: "",
                executionStage: "FIRST_PHASE_MALL_OWNED",
                mappingOperationId: expect.stringMatching(
                    /^w17:confirm-mapping:/,
                ),
            }),
        )
        expect(result.current.result).toMatchObject({
            status: "succeeded",
            title: "映射已确认",
            facts: [{ label: "已确认目标", value: "C-001 客户一" }],
        })
        expect(result.current.confirmOpen).toBe(false)
        expect(patchUrl).toHaveBeenCalledWith({
            view: "mapping",
            mappingTaskId: "mt-2",
            workItemId: "wi-1",
        })
    })

    it("surfaces a failed confirmation", async () => {
        apiMocks.confirm.mockResolvedValue({
            status: "failed",
            code: "X",
            message: "拒绝",
        })
        setPageData(makePageData())
        const { result } = renderPage()
        act(() => {
            result.current.setSelectedCandidateId("obj-1")
        })
        await act(async () => {
            await result.current.handleConfirm()
        })
        expect(result.current.actionError).toBe("拒绝")
        expect(result.current.result).toBeNull()
    })

    it("requires an eligible candidate before confirming", async () => {
        setPageData(makePageData())
        const { result } = renderPage()
        await act(async () => {
            await result.current.handleConfirm()
        })
        expect(apiMocks.confirm).not.toHaveBeenCalled()
        expect(result.current.actionError).toBe(
            "请选择可用的 ERP 候选（相似不自动确认）",
        )
    })

    it("submits a source fix request from the form", async () => {
        apiMocks.sourceFix.mockResolvedValue({
            status: "succeeded",
            mappingTaskId: "mt-1",
            mappingTaskStatus: "PENDING",
            workItemStatus: "OPEN",
            taskVersion: "6",
            mappingEvidenceEntryId: "ev-1",
            recordedAt: "2026-08-14T00:00:00Z",
            message: "已记录",
        })
        setPageData(makePageData())
        const { result } = renderPage()
        act(() => {
            result.current.sourceFixForm.setFieldValue("reasonCode", "OTHER")
            result.current.sourceFixForm.setFieldValue("note", "补充说明")
            result.current.sourceFixForm.setFieldValue(
                "requestedEvidence",
                "a，b\nc",
            )
        })
        await act(async () => {
            await result.current.sourceFixForm.handleSubmit()
        })
        expect(apiMocks.sourceFix).toHaveBeenCalledWith(
            expect.objectContaining({
                reasonCode: "OTHER",
                reasonText: "补充说明",
                requestedEvidence: ["a", "b", "c"],
                requestOperationId: expect.stringMatching(
                    /^w17:request-source-fix:/,
                ),
            }),
        )
        expect(result.current.sourceFixOpen).toBe(false)
        expect(result.current.result).toMatchObject({
            status: "succeeded",
            title: "来源修复说明已记录",
            reference: "ev-1",
        })
    })

    it("blocks source fix requests without current responsibility", async () => {
        const other = makeMappingTask()
        configuredWorkItem(other).ownerUser = {
            id: "user-9",
            displayName: "李四",
        }
        setPageData(makePageData({ selectedMappingTask: other }))
        const { result } = renderPage()
        await act(async () => {
            await result.current.sourceFixForm.handleSubmit()
        })
        expect(apiMocks.sourceFix).not.toHaveBeenCalled()
        expect(result.current.actionError).toBe(
            "当前责任不允许提交来源修复说明",
        )
    })

    it("triggers a single-order pull from the form", async () => {
        apiMocks.triggerSo.mockResolvedValue({
            status: "succeeded",
            jobId: "job-9",
            jobNo: "J-009",
            message: "已受理",
        })
        const patchUrl = vi.fn()
        setPageData(makePageData())
        const { result } = renderPage(makeInput({ patchUrl }))
        act(() => {
            result.current.pullForm.setFieldValue("externalOrderNo", "SO-123")
            result.current.pullForm.setFieldValue("reason", "漏单")
        })
        await act(async () => {
            await result.current.pullForm.handleSubmit()
        })
        expect(apiMocks.triggerSo).toHaveBeenCalledWith({
            externalOrderNo: "SO-123",
            reason: "漏单",
            stage: "FIRST_PHASE_MALL_OWNED",
            idempotencyKey: expect.stringMatching(/^w17:single-order:SO-123:/),
        })
        expect(result.current.pullOpen).toBe(false)
        expect(result.current.result).toMatchObject({
            status: "succeeded",
            title: "按单补拉已受理",
            reference: "J-009",
        })
        expect(patchUrl).toHaveBeenCalledWith({
            view: "jobs",
            jobId: "job-9",
        })
    })

    it("triggers a manual incremental from the form", async () => {
        apiMocks.triggerInc.mockResolvedValue({
            status: "succeeded",
            jobId: "job-8",
            jobNo: "J-008",
            message: "已受理",
        })
        const patchUrl = vi.fn()
        setPageData(makePageData())
        const { result } = renderPage(makeInput({ patchUrl }))
        act(() => {
            result.current.incrementalForm.setFieldValue("reason", "手动推进")
        })
        await act(async () => {
            await result.current.incrementalForm.handleSubmit()
        })
        expect(apiMocks.triggerInc).toHaveBeenCalledWith({
            reason: "手动推进",
            stage: "FIRST_PHASE_MALL_OWNED",
            idempotencyKey: expect.stringMatching(/^w17:incremental:manual:/),
        })
        expect(result.current.incrementalOpen).toBe(false)
        expect(patchUrl).toHaveBeenCalledWith({
            view: "jobs",
            jobId: "job-8",
        })
    })

    it("reapplies and reports success, unknown, and failure", async () => {
        apiMocks.reapply
            .mockResolvedValueOnce({
                status: "succeeded",
                operationId: "op-1",
                reapplyOperationStatus: "SUCCEEDED",
                salesOrderId: "so-1",
                salesOrderNo: "SO-1",
                salesOrderRevisionId: "r1",
                message: "ok",
            })
            .mockResolvedValueOnce({
                status: "unknown",
                reapplyOperationStatus: "UNKNOWN",
                operationId: "op-2",
                message: "结果未知",
                idempotencyKey: "ik-2",
            })
            .mockResolvedValueOnce({
                status: "failed",
                code: "X",
                message: "失败",
                operationId: "op-3",
                reapplyOperationStatus: "FAILED",
            })
        setPageData(makePageData())
        const { result } = renderPage()

        await act(async () => {
            await result.current.handleReapply()
        })
        expect(result.current.result).toMatchObject({
            status: "succeeded",
            title: "重新归集成功",
            reference: "SO-1",
        })

        await act(async () => {
            await result.current.handleReapply()
        })
        expect(result.current.result).toMatchObject({
            status: "unknown",
            title: "重新归集结果未知",
            pendingIdempotencyKey: "ik-2",
            reference: "op-2",
        })

        await act(async () => {
            await result.current.handleReapply()
        })
        expect(result.current.actionError).toBe("失败")
    })

    it("does not reapply without a mapping task", async () => {
        setPageData(
            makePageData({
                selectedMappingTask: undefined,
                mappingTasks: [],
            }),
        )
        const { result } = renderPage()
        await act(async () => {
            await result.current.handleReapply()
        })
        expect(apiMocks.reapply).not.toHaveBeenCalled()
    })

    it("retries the selected job", async () => {
        apiMocks.retryJob.mockResolvedValue({
            status: "succeeded",
            jobId: "job-1",
            jobNo: "J-001",
            message: "已创建",
        })
        setPageData(makePageData({ selectedJob: makeJobRow() }))
        const { result } = renderPage()
        await act(async () => {
            await result.current.handleRetryJob()
        })
        expect(apiMocks.retryJob).toHaveBeenCalledWith({
            jobId: "job-1",
            reason: "重试未成功部分的分页",
            stage: "FIRST_PHASE_MALL_OWNED",
            idempotencyKey: expect.stringMatching(/^w17:retry-job:job-1:/),
        })
        expect(result.current.retryConfirmOpen).toBe(false)
        expect(result.current.result).toMatchObject({
            status: "succeeded",
            title: "重试已创建",
        })
    })

    it("does not retry without a selected job", async () => {
        setPageData(makePageData())
        const { result } = renderPage()
        await act(async () => {
            await result.current.handleRetryJob()
        })
        expect(apiMocks.retryJob).not.toHaveBeenCalled()
    })

    it("resolves an unknown reapply result", async () => {
        apiMocks.resolveReapply.mockResolvedValue({
            status: "succeeded",
            salesOrderId: "so-1",
            message: "已确认",
        })
        const task = makeMappingTask()
        task.reapplyOperation = {
            operationId: "op-1",
            status: "UNKNOWN",
            statusLabel: "结果未知",
            lastUpdatedAt: "2026-08-14T00:00:00Z",
        }
        setPageData(makePageData({ selectedMappingTask: task }))
        const { result } = renderPage()
        await act(async () => {
            await result.current.handleResolveUnknownReapply()
        })
        expect(apiMocks.resolveReapply).toHaveBeenCalledWith({
            mappingTaskId: "mt-1",
            operationId: "op-1",
            settle: true,
        })
        expect(result.current.result).toMatchObject({
            status: "succeeded",
            title: "重新归集结果已确认",
        })
    })

    it("does not resolve without a reapply operation", async () => {
        setPageData(makePageData())
        const { result } = renderPage()
        await act(async () => {
            await result.current.handleResolveUnknownReapply()
        })
        expect(apiMocks.resolveReapply).not.toHaveBeenCalled()
    })
})
