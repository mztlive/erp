import { afterEach, expect, it, vi } from "vitest"
import { exportCustomerDirectory } from "./directory-export"

const fetchCustomerDirectory = vi.fn()

vi.mock("./directory", () => ({
    fetchCustomerDirectory: (...args: unknown[]) =>
        fetchCustomerDirectory(...args),
}))

afterEach(() => {
    fetchCustomerDirectory.mockReset()
})

it("跨页携带 scopeVersion，并在完整结果后撤权重验", async () => {
    fetchCustomerDirectory
        .mockResolvedValueOnce({
            items: [
                {
                    id: "c-1",
                    customerNo: "C-1",
                    legalName: "华",
                    statusLabel: { label: "启用" },
                    ownerName: "张三",
                    collaboratorCount: 0,
                },
            ],
            totalInScope: 1,
            scopeVersion: "v9",
        })
        .mockResolvedValueOnce({
            items: [
                {
                    id: "c-1",
                    customerNo: "C-1",
                    legalName: "华",
                    statusLabel: { label: "启用" },
                    ownerName: "张三",
                    collaboratorCount: 0,
                },
            ],
            totalInScope: 1,
            scopeVersion: "v9",
        })
    const csv = await exportCustomerDirectory({
        scope: "mine",
        status: "active",
        page: 1,
        pageSize: 20,
        orgUnitIds: "org-1",
    })
    expect(fetchCustomerDirectory).toHaveBeenNthCalledWith(1, {
        scope: "mine",
        status: "active",
        page: 1,
        pageSize: 100,
        orgUnitIds: "org-1",
        scopeVersion: undefined,
    })
    expect(fetchCustomerDirectory).toHaveBeenNthCalledWith(2, {
        scope: "mine",
        status: "active",
        page: 1,
        pageSize: 1,
        orgUnitIds: "org-1",
        scopeVersion: "v9",
    })
    expect(csv).toContain("C-1")
})

it("导出最后一页撤权时拒绝生成文件", async () => {
    const changed = Object.assign(new Error("数据范围已变化，请从第一页刷新"), {
        status: 409,
        code: "DATA_SCOPE_CHANGED",
    })
    fetchCustomerDirectory
        .mockResolvedValueOnce({
            items: [
                {
                    id: "c-1",
                    customerNo: "C-1",
                    legalName: "华",
                    statusLabel: { label: "启用" },
                    ownerName: "张三",
                    collaboratorCount: 0,
                },
            ],
            totalInScope: 2,
            scopeVersion: "v9",
        })
        .mockRejectedValueOnce(changed)
    await expect(
        exportCustomerDirectory({
            scope: "mine",
            status: "active",
            page: 1,
            pageSize: 20,
        }),
    ).rejects.toMatchObject({
        status: 409,
        code: "DATA_SCOPE_CHANGED",
    })
})

it("导出完成后的撤权重验失败时拒绝", async () => {
    const changed = Object.assign(new Error("数据范围已变化，请从第一页刷新"), {
        status: 409,
        code: "DATA_SCOPE_CHANGED",
    })
    fetchCustomerDirectory
        .mockResolvedValueOnce({
            items: [
                {
                    id: "c-1",
                    customerNo: "C-1",
                    legalName: "华",
                    statusLabel: { label: "启用" },
                    ownerName: "张三",
                    collaboratorCount: 0,
                },
            ],
            totalInScope: 1,
            scopeVersion: "v9",
        })
        .mockRejectedValueOnce(changed)
    await expect(
        exportCustomerDirectory({
            scope: "all_authorized",
            status: "active",
            page: 1,
            pageSize: 20,
        }),
    ).rejects.toMatchObject({ code: "DATA_SCOPE_CHANGED" })
})
