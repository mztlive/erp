import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { renderHook, waitFor, cleanup } from "@testing-library/react"
import type { PropsWithChildren } from "react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { useWorkItemsQuery, workItemKeys } from "./queries"
import { listWorkItems, type WorkItemListParams } from "./api"

vi.mock("./api", () => ({
    listWorkItems: vi.fn(),
    getWorkItem: vi.fn(),
    getWorkItemStats: vi.fn(),
    getWorkItemReassignCandidates: vi.fn(),
    parseWorkItemConflict: vi.fn(),
    submitWorkItemResponsibility: vi.fn(),
}))

afterEach(cleanup)
beforeEach(() => vi.clearAllMocks())

const page = (version: string) => ({
    items: [],
    total: 0,
    page: 1,
    page_size: 1,
    scope_version: version,
})
const params: WorkItemListParams = {
    scope: "mine",
    timezone: "Asia/Shanghai",
    page: 2,
    pageSize: 1,
}
function setup() {
    const client = new QueryClient({
        defaultOptions: { queries: { retry: false } },
    })
    const wrapper = ({ children }: PropsWithChildren) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
    )
    return { client, wrapper }
}

describe("任务队列跨页范围版本", () => {
    it("直接进入后续页时先读取第一页取得范围锚点", async () => {
        vi.mocked(listWorkItems).mockResolvedValue(page("v1"))
        const { wrapper } = setup()
        const { result } = renderHook(() => useWorkItemsQuery(params), {
            wrapper,
        })
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(listWorkItems).toHaveBeenNthCalledWith(1, {
            ...params,
            page: 1,
            scopeVersion: undefined,
        })
        expect(listWorkItems).toHaveBeenNthCalledWith(2, {
            ...params,
            scopeVersion: "v1",
        })
    })
    it("缓存锚点进入请求和 Query key；范围冲突不得自动接收新的后续页", async () => {
        const { client, wrapper } = setup()
        client.setQueryData(
            workItemKeys.list({ ...params, page: 1, scopeVersion: undefined }),
            page("v1"),
        )
        const error = Object.assign(
            new Error("数据范围已变化，请从第一页刷新"),
            { code: "DATA_SCOPE_CHANGED", status: 409 },
        )
        vi.mocked(listWorkItems).mockRejectedValue(error)
        const { result } = renderHook(() => useWorkItemsQuery(params), {
            wrapper,
        })
        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBe(error)
        expect(result.current.data).toBeUndefined()
        expect(listWorkItems).toHaveBeenCalledTimes(1)
        expect(
            client.getQueryState(
                workItemKeys.list({ ...params, scopeVersion: "v1" }),
            )?.status,
        ).toBe("error")
    })
})
