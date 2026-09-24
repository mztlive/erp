import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, renderHook, waitFor } from "@testing-library/react"
import type { ReactNode } from "react"
import { afterEach, beforeEach, expect, test, vi } from "vitest"
import { apiGet } from "@/lib/api"
import {
    searchObjectDirectory,
    selectedObjectDirectory,
} from "@/lib/object-directory"
import { useHistoricalDirectory } from "@/lib/historical-directory"
import { usePersonDirectoryList } from "@/features/entity-selectors/hooks/person-directory"

vi.mock("@/lib/api", () => ({ apiGet: vi.fn() }))
beforeEach(() => vi.resetAllMocks())
afterEach(cleanup)

function wrapper() {
    const client = new QueryClient({
        defaultOptions: { queries: { retry: false } },
    })
    return ({ children }: { children: ReactNode }) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
    )
}
const party = { id: "p1", code: "P1", name: "无合同主体", status: "disabled" }

test("主体目录完整读取停用对象，后续页只携带自身版本", async () => {
    vi.mocked(apiGet)
        .mockResolvedValueOnce({
            items: [party],
            total: 2,
            scope_version: "dir-v1",
        })
        .mockResolvedValueOnce({
            items: [{ ...party, id: "p2" }],
            total: 2,
            scope_version: "dir-v1",
        })
    expect(
        await searchObjectDirectory("settlement-parties", " 无合同 "),
    ).toEqual({
        items: [party, { ...party, id: "p2" }],
        total: 2,
        scope_version: "dir-v1",
    })
    expect(apiGet).toHaveBeenNthCalledWith(1, "/admin/settlement-parties", {
        q: "无合同",
        page: 1,
        page_size: 100,
    })
    expect(apiGet).toHaveBeenNthCalledWith(2, "/admin/settlement-parties", {
        q: "无合同",
        page: 2,
        page_size: 100,
        scope_version: "dir-v1",
    })
})

test("目录跨页版本变化时整次失败，不返回第一页残留", async () => {
    vi.mocked(apiGet)
        .mockResolvedValueOnce({
            items: [party],
            total: 2,
            scope_version: "old",
        })
        .mockResolvedValueOnce({
            items: [{ ...party, id: "p2" }],
            total: 2,
            scope_version: "new",
        })
    await expect(
        searchObjectDirectory("settlement-parties", ""),
    ).rejects.toMatchObject({ code: "DATA_SCOPE_CHANGED" })
})

test("目录超过一万项时要求收窄查询，不截断返回", async () => {
    vi.mocked(apiGet).mockResolvedValue({
        items: [party],
        total: 10001,
        scope_version: "v1",
    })
    await expect(
        searchObjectDirectory("warehouse-directory", ""),
    ).rejects.toMatchObject({ kind: "Validation" })
    expect(apiGet).toHaveBeenCalledTimes(1)
})

test("已选回显使用自身目录，空范围保留信封，请求失败保持失败", async () => {
    expect(await selectedObjectDirectory("settlement-parties", "")).toBeNull()
    expect(apiGet).not.toHaveBeenCalled()
    const empty = {
        items: [],
        total: 0,
        empty_reason: "no_scope",
        scope_version: "dir-v1",
        policy_version: 2,
        organization_version: 3,
        as_of: "2026-09-23T00:00:00Z",
    }
    vi.mocked(apiGet).mockResolvedValueOnce(empty)
    expect(await selectedObjectDirectory("settlement-parties", "p1")).toEqual(
        empty,
    )
    expect(apiGet).toHaveBeenCalledWith("/admin/settlement-parties/selected", {
        ids: "p1",
    })
    const denied = { status: 403, message: "无目录权限" }
    vi.mocked(apiGet).mockRejectedValueOnce(denied)
    await expect(
        selectedObjectDirectory("settlement-parties", "p1"),
    ).rejects.toBe(denied)
})

test("人员分页沿用独立目录版本，不请求关联业务列表", async () => {
    const person = {
        id: "sales1",
        name: "无合同销售",
        account: "sales1",
        status: "active",
    }
    vi.mocked(apiGet)
        .mockResolvedValueOnce({
            items: [person],
            total: 2,
            page_size: 1,
            scope_version: "people-v1",
        })
        .mockResolvedValueOnce({
            items: [{ ...person, id: "sales2" }],
            total: 2,
            page_size: 1,
            scope_version: "people-v1",
        })
    const { result } = renderHook(
        () =>
            usePersonDirectoryList({
                category: "sales",
                q: "",
                orgUnitIds: [],
                includeDescendants: true,
                pageCount: 2,
            }),
        { wrapper: wrapper() },
    )
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(
        result.current.data?.pages.flatMap((page) => page.items),
    ).toHaveLength(2)
    expect(apiGet).toHaveBeenCalledTimes(2)
    expect(apiGet).toHaveBeenLastCalledWith("/admin/salespeople", {
        q: undefined,
        page: 2,
        page_size: 20,
        org_unit_ids: undefined,
        include_descendants: undefined,
        scope_version: "people-v1",
    })
})

test("报表页码、状态、人员筛选变化不重新请求历史身份目录", async () => {
    vi.mocked(apiGet).mockResolvedValue({
        attributionUserOptions: [],
        attributionOrgOptions: [],
        scopeVersion: "history-v1",
    })
    const initial = {
        from: "2026-01-01",
        to: "2026-01-31",
        customerId: "c1",
        periodBasis: "FULFILLMENT",
        page: 1,
        attributionUserIds: "u1",
        status: "open",
    }
    const { result, rerender } = renderHook(
        (input) =>
            useHistoricalDirectory(
                "/admin/actual-profit-loss/history-directory",
                input,
            ),
        { initialProps: initial, wrapper: wrapper() },
    )
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    rerender({
        ...initial,
        page: 2,
        attributionUserIds: "u2",
        status: "closed",
    })
    expect(apiGet).toHaveBeenCalledTimes(1)
    expect(apiGet).toHaveBeenCalledWith(
        "/admin/actual-profit-loss/history-directory",
        {
            from: initial.from,
            to: initial.to,
            customer_id: "c1",
            period_basis: "FULFILLMENT",
        },
    )
    rerender({ ...initial, customerId: "c2" })
    await waitFor(() => expect(apiGet).toHaveBeenCalledTimes(2))
})
