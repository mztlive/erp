import { expect, it, vi } from "vitest"
import { buildListCsv, collectExportPages } from "@/lib/list-export"

it("跨三页导出 201 条，单页最多 100，尾页不会被截断", async () => {
    const source = Array.from({ length: 201 }, (_, id) => ({ id: String(id) }))
    const load = vi.fn(async (page: number, size: number) => ({
        items: source.slice((page - 1) * size, page * size),
        total: source.length,
    }))
    const result = await collectExportPages(load, (row) => row.id)
    expect(result).toEqual(source)
    expect(load.mock.calls).toEqual([
        [1, 100],
        [2, 100],
        [3, 100],
    ])
})

it("失败、总数改变、重复行或空尾页均拒绝部分导出", async () => {
    const first = { items: [{ id: "one" }], total: 2 }
    for (const second of [
        { items: [], total: 2 },
        { items: [{ id: "one" }], total: 2 },
        { items: [{ id: "two" }], total: 3 },
    ]) {
        const load = vi
            .fn()
            .mockResolvedValueOnce(first)
            .mockResolvedValueOnce(second)
        await expect(
            collectExportPages(load, (row: { id: string }) => row.id),
        ).rejects.toMatchObject({ kind: "Validation" })
    }
    const forbidden = { kind: "Auth", message: "权限已变化" }
    await expect(
        collectExportPages(
            vi
                .fn()
                .mockResolvedValueOnce(first)
                .mockRejectedValueOnce(forbidden),
            (row: { id: string }) => row.id,
        ),
    ).rejects.toBe(forbidden)
})

it("空结果结束，CSV保留中文和换行并转义公式", async () => {
    expect(
        await collectExportPages(async () => ({ items: [], total: 0 }), String),
    ).toEqual([])
    expect(buildListCsv([["张三", 'a"b', "=SUM(A1)"]])).toBe(
        '\uFEFF"张三","a""b","\'=SUM(A1)"',
    )
})
