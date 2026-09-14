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
