import { expect, it, vi } from "vitest"
import { collectQueuePages } from "./queue-pagination"

it("按接口 total 继续翻页，保留首次加载范围之外的结果", async () => {
    const source = Array.from({ length: 121 }, (_, i) => ({
        id: `task-${i + 1}`,
    }))
    const load = vi.fn(async (page: number) => ({
        items: source.slice((page - 1) * 50, page * 50),
        total: source.length,
        page,
        page_size: 50,
    }))
    const result = await collectQueuePages(load)
    expect(load).toHaveBeenCalledTimes(3)
    expect(result.at(-1)?.id).toBe("task-121")
    expect(result).toHaveLength(121)
})

it("分页不前进或后续请求失败时不返回部分成功队列", async () => {
    const samePage = vi.fn(async (page: number) => ({
        items: [{ id: "same" }],
        total: 5,
        page,
        page_size: 1,
    }))
    await expect(collectQueuePages(samePage)).rejects.toThrow("请重新查询")
    const failLater = vi
        .fn()
        .mockResolvedValueOnce({ items: [{ id: "first" }], total: 2 })
        .mockRejectedValueOnce(new Error("服务不可用"))
    await expect(collectQueuePages(failLater)).rejects.toThrow("服务不可用")
})

it("空查询结果正常返回空队列", async () => {
    await expect(
        collectQueuePages(async () => ({
            items: [],
            total: 0,
            page: 1,
            page_size: 50,
        })),
    ).resolves.toEqual([])
})
