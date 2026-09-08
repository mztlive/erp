import { beforeEach, describe, expect, it, vi } from "vitest"
import { fetchAllPages } from "./fetch-all"
import { listVoucherCategories } from "./voucher"

vi.mock("./fetch-all", () => ({ fetchAllPages: vi.fn() }))

beforeEach(() => {
    vi.mocked(fetchAllPages).mockReset()
    vi.mocked(fetchAllPages).mockImplementation(async (path) =>
        path === "/admin/voucher-category-profiles"
            ? [
                  {
                      id: "old",
                      sku_id: "sku-1",
                      sku_no: "VC-1",
                      name: "已停用类目",
                      revision_no: 1,
                      description: "旧版",
                      status: "active",
                      created_at: 1700000000,
                      version: 1,
                  },
                  {
                      id: "current",
                      sku_id: "sku-1",
                      sku_no: "VC-1",
                      name: "已停用类目",
                      revision_no: 2,
                      description: "停用版",
                      status: "disabled",
                      created_at: 1700100000,
                      version: 2,
                  },
                  {
                      id: "other",
                      sku_id: "sku-2",
                      sku_no: "VC-2",
                      name: "启用类目",
                      revision_no: 1,
                      description: "启用版",
                      status: "active",
                      created_at: 1700000000,
                      version: 1,
                  },
              ]
            : [],
    )
})

describe("voucher lifecycle filtering", () => {
    it("filters after choosing the latest revision so old enabled versions cannot resurrect disabled categories", async () => {
        const enabled = await listVoucherCategories({
            resource: "voucher-categories",
            lifecycleStatus: "enabled",
        })
        const disabled = await listVoucherCategories({
            resource: "voucher-categories",
            lifecycleStatus: "disabled",
        })
        const all = await listVoucherCategories({
            resource: "voucher-categories",
            lifecycleStatus: "all",
        })
        expect(enabled.map((row) => row.stableId)).toEqual(["sku-2"])
        expect(disabled.map((row) => row.currentRevisionId)).toEqual([
            "current",
        ])
        expect(all).toHaveLength(2)
        expect(disabled[0].allowedActions).toContain("ENABLE")
        expect(disabled[0].allowedActions).not.toContain("DISABLE")
        expect(enabled[0].allowedActions).toContain("DISABLE")
        expect(fetchAllPages).toHaveBeenCalledWith(
            "/admin/voucher-category-profiles",
            {},
        )
    })
    it("propagates failure instead of reporting a misleading empty status tab", async () => {
        vi.mocked(fetchAllPages).mockRejectedValueOnce(new Error("offline"))
        await expect(
            listVoucherCategories({ resource: "voucher-categories" }),
        ).rejects.toThrow("offline")
    })
})
