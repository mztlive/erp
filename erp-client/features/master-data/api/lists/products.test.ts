import { beforeEach, describe, expect, it, vi } from "vitest"

const fetchCompleteList = vi.fn()
const fetchAllPages = vi.hoisted(() => vi.fn())

vi.mock("@/lib/collect-pages", () => ({
    fetchCompleteList: (...args: unknown[]) => fetchCompleteList(...args),
}))

vi.mock("./fetch-all", () => ({
    fetchAllPages,
}))

import { listProducts, fetchProductFilterOptions } from "./products"

describe("listProducts", () => {
    beforeEach(() => {
        fetchCompleteList.mockReset()
        fetchCompleteList.mockResolvedValue({
            items: [
                {
                    id: "p-1",
                    product_no: "P-1",
                    product_kind: "PHYSICAL",
                    status: "active",
                    listing_status: "listed",
                    listed_sku_count: 1,
                    sku_count: 1,
                    current_revision_id: "r-1",
                    created_at: 1,
                    version: 1,
                    maintainer_user_id: "user-1",
                    business_org_unit_id: "org-1",
                },
            ],
            total: 1,
            empty_reason: null,
            owner_options: [{ value: "user-1", label: "张三" }],
            procurement_owner_options: [{ value: "buyer-1", label: "李四" }],
        })
    })

    it("把维护人与采购负责人筛选送进同一授权查询", async () => {
        const result = await listProducts({
            resource: "products",
            ownerUserIds: "user-1",
            procurementOwnerUserIds: "buyer-1",
            orgUnitIds: "org-1",
            includeDescendants: true,
        })
        expect(fetchCompleteList).toHaveBeenCalledWith(
            "/admin/products",
            expect.objectContaining({
                owner_user_ids: "user-1",
                procurement_owner_user_ids: "buyer-1",
                org_unit_ids: "org-1",
                include_descendants: true,
            }),
        )
        expect(result.rows[0]?.ownerUserId).toBe("user-1")
        expect(result.ownerOptions).toEqual([
            { value: "user-1", label: "张三" },
        ])
        expect(result.procurementOwnerOptions[0]?.value).toBe("buyer-1")
        expect(result.emptyReason).toBeNull()
    })
})

describe("product filter permissions", () => {
    beforeEach(() => {
        fetchAllPages.mockReset()
        fetchAllPages.mockResolvedValue([])
    })
    it("sales can load the product pool without reading category, brand or supplier administration", async () => {
        expect(
            await fetchProductFilterOptions([
                "sellable_sku:list",
                "product:detail",
                "contract:*",
            ]),
        ).toEqual({
            categories: [],
            brands: [],
            suppliers: [],
            unavailable: ["categories", "brands", "suppliers"],
        })
        expect(fetchAllPages).not.toHaveBeenCalled()
    })
    it("requests only granted option resources", async () => {
        fetchAllPages.mockResolvedValue([
            { id: "cat-1", category_code: "C1", name: "食品" },
        ])
        const result = await fetchProductFilterOptions([
            "product_category:list",
        ])
        expect(fetchAllPages).toHaveBeenCalledTimes(1)
        expect(fetchAllPages).toHaveBeenCalledWith(
            "/admin/product-categories",
            expect.any(Object),
        )
        expect(result.categories[0]?.categoryName).toBe("食品")
        expect(result.brands).toEqual([])
        expect(result.suppliers).toEqual([])
    })
    it("retains all option queries for wildcard administrators", async () => {
        await fetchProductFilterOptions(["*:*"])
        expect(fetchAllPages.mock.calls.map(([path]) => path)).toEqual([
            "/admin/product-categories",
            "/admin/product-brands",
            "/admin/suppliers",
        ])
    })
    it("does not conceal a server rejection of a granted resource", async () => {
        const error = Object.assign(new Error("权限已变化"), { status: 403 })
        fetchAllPages.mockRejectedValue(error)
        await expect(
            fetchProductFilterOptions(["product_brand:list"]),
        ).rejects.toBe(error)
    })
})
