import { afterEach, expect, it, vi } from "vitest"
import { createContractExportJob } from "./export"

const fetchContracts = vi.fn()

vi.mock("./list", () => ({
    fetchContracts: (...args: unknown[]) => fetchContracts(...args),
}))

afterEach(() => {
    fetchContracts.mockReset()
})

it("跨页携带 scopeVersion，并在完整结果后撤权重验", async () => {
    fetchContracts
        .mockResolvedValueOnce({
            items: [
                {
                    contractId: "c-1",
                    contractNo: "HT-1",
                    customer: { displayName: "华" },
                    settlementParty: { displayName: "结算" },
                    statusLabel: "生效",
                    ownerLabel: "张三",
                    validFrom: "2026-01-01",
                    validTo: "2026-12-31",
                },
            ],
            total: 1,
            scopeVersion: "v9",
        })
        .mockResolvedValueOnce({
            items: [
                {
                    contractId: "c-1",
                    contractNo: "HT-1",
                    customer: { displayName: "华" },
                    settlementParty: { displayName: "结算" },
                    statusLabel: "生效",
                    ownerLabel: "张三",
                    validFrom: "2026-01-01",
                    validTo: "2026-12-31",
                },
            ],
            total: 1,
            scopeVersion: "v9",
        })
    const job = await createContractExportJob({
        query: {
            metric: "all",
            page: 1,
            pageSize: 20,
            orgUnitIds: "org-1",
            includeDescendants: true,
        },
        filterSnapshotLabel: "组织=org-1",
    })
    expect(fetchContracts).toHaveBeenNthCalledWith(1, {
        metric: "all",
        page: 1,
        pageSize: 100,
        orgUnitIds: "org-1",
        includeDescendants: true,
        scopeVersion: undefined,
    })
    expect(fetchContracts).toHaveBeenNthCalledWith(2, {
        metric: "all",
        page: 1,
        pageSize: 1,
        orgUnitIds: "org-1",
        includeDescendants: true,
        scopeVersion: "v9",
    })
    expect(job.content).toContain("HT-1")
    expect(job.rowCount).toBe(1)
})

it("导出最后一页撤权时拒绝生成文件", async () => {
    const changed = Object.assign(new Error("数据范围已变化，请从第一页刷新"), {
        status: 409,
        code: "DATA_SCOPE_CHANGED",
    })
    fetchContracts
        .mockResolvedValueOnce({
            items: [
                {
                    contractId: "c-1",
                    contractNo: "HT-1",
                    customer: { displayName: "华" },
                    settlementParty: { displayName: "结算" },
                    statusLabel: "生效",
                    ownerLabel: "张三",
                    validFrom: "2026-01-01",
                    validTo: "2026-12-31",
                },
            ],
            total: 2,
            scopeVersion: "v9",
        })
        .mockRejectedValueOnce(changed)
    await expect(
        createContractExportJob({
            query: {
                metric: "all",
                page: 1,
                pageSize: 20,
                includeDescendants: false,
            },
            filterSnapshotLabel: "全部",
        }),
    ).rejects.toMatchObject({
        status: 409,
        code: "DATA_SCOPE_CHANGED",
    })
})
