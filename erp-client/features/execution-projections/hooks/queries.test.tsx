import { describe, it, expect, vi, beforeEach } from "vitest"
import { renderHook, waitFor, act } from "@testing-library/react"
import { QueryClientProvider } from "@tanstack/react-query"
import type { ReactNode } from "react"

import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import type { QueryClient } from "@tanstack/react-query"

vi.mock("../api/projections", () => ({
    fetchExecutionProjectionList: vi.fn(),
    fetchExecutionProjectionDetail: vi.fn(),
    fetchSalesOrderCollaboration: vi.fn(),
    submitProjectionDeliveryCommand: vi.fn(),
    submitBulkProjectionCommand: vi.fn(),
}))

import {
    fetchExecutionProjectionDetail,
    fetchExecutionProjectionList,
    fetchSalesOrderCollaboration,
    submitBulkProjectionCommand,
    submitProjectionDeliveryCommand,
} from "../api/projections"
import {
    useBulkProjectionCommandMutation,
    useExecutionProjectionDetailQuery,
    useExecutionProjectionListQuery,
    useProjectionDeliveryCommandMutation,
    useSalesOrderCollaborationQuery,
} from "./queries"
import type {
    ExecutionProjectionListResult,
    ProjectionDeliveryCommandResult,
} from "../types"

const mockedFetchList = vi.mocked(fetchExecutionProjectionList)
const mockedFetchDetail = vi.mocked(fetchExecutionProjectionDetail)
const mockedFetchCollaboration = vi.mocked(fetchSalesOrderCollaboration)
const mockedSubmitCommand = vi.mocked(submitProjectionDeliveryCommand)
const mockedSubmitBulk = vi.mocked(submitBulkProjectionCommand)

function listResult(): ExecutionProjectionListResult {
    return {
        rows: [],
        pageInfo: { page: 1, pageSize: 20, total: 0 },
        metrics: [],
        malls: [],
        permissionVersion: "pv-live",
        sourceFactsAsOf: "2026-08-01T00:00:00.000Z",
        projectionUpdatedAt: "2026-08-01T00:00:00.000Z",
        deliveryStatusUpdatedAt: "2026-08-01T00:00:00.000Z",
        queriedAt: "2026-08-01T00:00:00.000Z",
        filterSummary: "默认：风险优先 · 全状态",
        defaultViewNote: "运营默认关注未确认与失败；结果未知不计入已确认指标。",
    }
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("useExecutionProjectionListQuery", () => {
    it("查询期间先挂起，随后返回数据并把 query 传给 API", async () => {
        const query = { page: 2, pageSize: 20, mallId: "mall-1" }
        mockedFetchList.mockResolvedValue(listResult())

        const { result } = renderHookWithProviders(() =>
            useExecutionProjectionListQuery(query),
        )

        expect(result.current.isPending).toBe(true)
        await waitFor(() => expect(result.current.isPending).toBe(false))
        expect(result.current.data).toEqual(listResult())
        expect(mockedFetchList).toHaveBeenCalledWith(query)
    })

    it("同值不同引用的 query 参数保持 queryKey 稳定，只请求一次", async () => {
        mockedFetchList.mockResolvedValue(listResult())
        const client = createFreshQueryClient()
        const wrapper = ({ children }: { children: ReactNode }) => (
            <QueryClientProvider client={client}>
                {children}
            </QueryClientProvider>
        )
        const { result, rerender } = renderHook(
            ({ q }: { q?: string }) =>
                useExecutionProjectionListQuery({ page: 1, pageSize: 20, q }),
            { wrapper, initialProps: { q: "SO-1" } },
        )

        rerender({ q: "SO-1" })
        await waitFor(() => expect(result.current.isSuccess).toBe(true))

        expect(mockedFetchList).toHaveBeenCalledTimes(1)
        expect(
            client.getQueryData<ExecutionProjectionListResult>([
                "execution-projections",
                "list",
                { page: 1, pageSize: 20, q: "SO-1" },
            ]),
        ).toEqual(listResult())
    })
})

describe("useExecutionProjectionDetailQuery", () => {
    it("没有 projectionId 时禁用查询且不发请求", () => {
        renderHookWithProviders(() =>
            useExecutionProjectionDetailQuery(undefined),
        )
        expect(mockedFetchDetail).not.toHaveBeenCalled()
    })

    it("传入 projectionId 与 revisionId 时按对应参数取详情", async () => {
        mockedFetchDetail.mockResolvedValue(null)
        const { result } = renderHookWithProviders(() =>
            useExecutionProjectionDetailQuery("proj-1", "rev-2"),
        )
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(mockedFetchDetail).toHaveBeenCalledWith({
            projectionId: "proj-1",
            revisionId: "rev-2",
        })
    })

    it("API 出错时进入 error 状态", async () => {
        mockedFetchDetail.mockRejectedValue(new Error("boom"))
        const { result } = renderHookWithProviders(() =>
            useExecutionProjectionDetailQuery("proj-1"),
        )
        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe("useSalesOrderCollaborationQuery", () => {
    it("空 salesOrderId 禁用，非空则请求协同摘要", async () => {
        mockedFetchCollaboration.mockResolvedValue({
            salesOrderId: "SO-1",
            salesOrderNo: "SO-1",
            hasProjection: false,
            historyCount: 0,
            note: "当前销售单尚无执行信息。卡券销售版本生效后由系统自动形成数据。",
        })

        const client = createFreshQueryClient()
        const wrapper = ({ children }: { children: ReactNode }) => (
            <QueryClientProvider client={client}>
                {children}
            </QueryClientProvider>
        )
        const { result, rerender } = renderHook(
            ({ id }: { id: string }) => useSalesOrderCollaborationQuery(id),
            { wrapper, initialProps: { id: "" } },
        )
        expect(mockedFetchCollaboration).not.toHaveBeenCalled()

        rerender({ id: "SO-1" })
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(mockedFetchCollaboration).toHaveBeenCalledWith("SO-1")
    })
})

describe("useProjectionDeliveryCommandMutation", () => {
    let client: QueryClient

    beforeEach(() => {
        client = createFreshQueryClient()
    })

    it("mutationFn 调用投递命令 API，成功后失效全部 execution-projections 缓存", async () => {
        const invalidateSpy = vi.spyOn(client, "invalidateQueries")
        const input = {
            projectionId: "p1",
            projectionRevisionId: "r1",
            deliveryId: "d1",
            action: "QUERY_RESULT" as const,
            expectedObjectVersion: "3",
            requestId: "req-1",
        }
        mockedSubmitCommand.mockResolvedValue({
            operationId: "op-1",
            deliveryId: "d1",
            projectionId: "p1",
            salesOrderNo: "p1",
            result: "ACKED",
            resultLabel: "已确认",
            occurredAt: "2026-08-01T00:00:00.000Z",
            nextAction: "无需进一步操作",
            stillUnknown: false,
            objectVersion: "3",
        })

        const { result } = renderHookWithProviders(
            () => useProjectionDeliveryCommandMutation(),
            { queryClient: client },
        )

        const outcomeHolder: {
            value: ProjectionDeliveryCommandResult | null
        } = { value: null }
        await act(async () => {
            outcomeHolder.value = await result.current.mutateAsync(input)
        })
        expect(mockedSubmitCommand).toHaveBeenCalledWith(input)
        expect(outcomeHolder.value?.result).toBe("ACKED")
        await waitFor(() =>
            expect(invalidateSpy).toHaveBeenCalledWith({
                queryKey: ["execution-projections"],
            }),
        )
    })
})

describe("useBulkProjectionCommandMutation", () => {
    it("mutationFn 调用批量命令 API，成功后失效执行投影缓存", async () => {
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, "invalidateQueries")
        const input = {
            action: "BULK_QUERY" as const,
            projectionIds: ["p1"],
            requestId: "bulk-1",
        }
        mockedSubmitBulk.mockResolvedValue({
            jobId: "bulk_BULK_QUERY_bulk-1",
            action: "BULK_QUERY",
            status: "failed",
            total: 0,
            completed: 0,
            succeeded: 0,
            skipped: 0,
            failed: 0,
            stillUnknown: 0,
            selectionSnapshotId: "snap-bulk-1",
            items: [],
            startedAt: "2026-08-01T00:00:00.000Z",
            nextAction: "请先逐项显式勾选失败/可处理项",
        })

        const { result } = renderHookWithProviders(
            () => useBulkProjectionCommandMutation(),
            { queryClient: client },
        )
        await act(async () => {
            await result.current.mutateAsync(input)
        })
        expect(mockedSubmitBulk).toHaveBeenCalledWith(input)
        await waitFor(() =>
            expect(invalidateSpy).toHaveBeenCalledWith({
                queryKey: ["execution-projections"],
            }),
        )
    })
})
