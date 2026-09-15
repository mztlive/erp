import { beforeEach, expect, it, vi } from "vitest"
import { apiGet } from "@/lib/api"
import { fetchCompleteList } from "@/lib/collect-pages"

vi.mock("@/lib/api", () => ({ apiGet: vi.fn() }))
beforeEach(() => vi.resetAllMocks())

it("跨过第 100 条并在每页保留关键词及结构化条件", async () => {
    const items = Array.from({ length: 101 }, (_, index) => ({
        id: String(index),
    }))
    vi.mocked(apiGet)
        .mockResolvedValueOnce({ items: items.slice(0, 100), total: 101 })
        .mockResolvedValueOnce({ items: items.slice(100), total: 101 })
    expect(
        await fetchCompleteList("/list", { q: "A.[1]", supplier_id: "s1" }),
    ).toEqual({ items, total: 101 })
    expect(apiGet).toHaveBeenNthCalledWith(2, "/list", {
        q: "A.[1]",
        supplier_id: "s1",
        page: 2,
        page_size: 100,
    })
})

it.each([
    { items: [{ id: "first" }], total: 2 },
    { items: [], total: 2 },
    { items: [{ id: "second" }], total: 3 },
])("重复页、空中间页和总数变化均拒绝返回部分数据 %#", async (second) => {
    vi.mocked(apiGet)
        .mockResolvedValueOnce({ items: [{ id: "first" }], total: 2 })
        .mockResolvedValueOnce(second)
    await expect(fetchCompleteList("/list")).rejects.toThrow("列表数据已变化")
})

it("后续请求失败时保留原始错误", async () => {
    const failure = new Error("network")
    vi.mocked(apiGet)
        .mockResolvedValueOnce({ items: [{ id: "first" }], total: 2 })
        .mockRejectedValueOnce(failure)
    await expect(fetchCompleteList("/list")).rejects.toBe(failure)
})

it("空结果只查询一次", async () => {
    vi.mocked(apiGet).mockResolvedValue({ items: [], total: 0 })
    expect(await fetchCompleteList("/list")).toEqual({ items: [], total: 0 })
    expect(apiGet).toHaveBeenCalledTimes(1)
})

it("跨页携带并复核 scope_version，版本变化拒绝拼接", async () => {
    const items = [{ id: "first" }, { id: "second" }]
    vi.mocked(apiGet)
        .mockResolvedValueOnce({
            items: [items[0]],
            total: 2,
            scope_version: "v1",
            empty_reason: null,
            policy_version: 3,
            organization_version: 4,
        })
        .mockResolvedValueOnce({
            items: [items[1]],
            total: 2,
            scope_version: "v2",
            policy_version: 3,
            organization_version: 4,
        })
    await expect(fetchCompleteList("/list")).rejects.toMatchObject({
        status: 409,
        code: "DATA_SCOPE_CHANGED",
    })
    expect(apiGet).toHaveBeenNthCalledWith(2, "/list", {
        page: 2,
        page_size: 100,
        scope_version: "v1",
    })
})

it("完整收集从信封保留版本字段", async () => {
    vi.mocked(apiGet).mockResolvedValue({
        items: [{ id: "one" }],
        total: 1,
        scope_version: "v9",
        empty_reason: "no_scope",
        policy_version: 2,
        organization_version: 8,
        as_of: "2026-09-15T00:00:00Z",
        scope_summary: "摘要",
        ownership_basis: "data_scope_configuration",
    })
    expect(await fetchCompleteList("/list")).toMatchObject({
        items: [{ id: "one" }],
        total: 1,
        scope_version: "v9",
        empty_reason: "no_scope",
        policy_version: 2,
        organization_version: 8,
    })
})
