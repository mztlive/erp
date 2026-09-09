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
