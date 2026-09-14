import { expect, it } from "vitest"
import {
    buildSalesOrdersSearchParams,
    mergeSalesOrdersSearchParams,
    normalizedSalesOrdersSearchParams,
    parseSalesOrdersSearchParams,
} from "@/features/sales-orders/lib/url-state"
import {
    filterDraftFromUrl,
    resolveSalesOrdersListFilterPatch,
} from "@/features/sales-orders/lib/sales-orders-list-filters"
import {
    buildPurchaseOrdersSearchParams,
    parsePurchaseOrdersSearchParams,
} from "@/features/purchase-orders/lib/url-state"
import { contractsUrlCodec } from "@/features/contracts/lib/contracts-url-state"
import { writeDirectoryUrl } from "@/features/customers/lib/directory-url"

it("销售负责人应用、刷新、分页与清除使用同一URL键，不积累重复参数", () => {
    const url = parseSalesOrdersSearchParams(
        new URLSearchParams("ownerUserIds=user-1,user-2&page=3"),
    )
    expect(url.ownerUserIds).toBe("user-1,user-2")
    expect(filterDraftFromUrl(url).ownerUserIds).toBe(url.ownerUserIds)
    const built = buildSalesOrdersSearchParams(url)
    expect(
        normalizedSalesOrdersSearchParams(new URLSearchParams(built), url),
    ).toBeUndefined()
    const applied = resolveSalesOrdersListFilterPatch({
        summary: "all",
        searchDraft: "客户",
        filterDraft: { ...filterDraftFromUrl(url), ownerUserIds: "user-2" },
    })
    expect(applied).toMatchObject({ page: 1, ownerUserIds: "user-2" })
    const cleared = mergeSalesOrdersSearchParams(new URLSearchParams(built), {
        ...url,
        ownerUserIds: undefined,
        page: 1,
    })
    expect(new URLSearchParams(cleared).has("ownerUserIds")).toBe(false)
})

it("销售组织筛选进入 URL 与查询草稿，含下级必须绑定组织", () => {
    const url = parseSalesOrdersSearchParams(
        new URLSearchParams("orgUnitIds=org-1&includeDescendants=1&page=2"),
    )
    expect(url.orgUnitIds).toBe("org-1")
    expect(url.includeDescendants).toBe(true)
    const applied = resolveSalesOrdersListFilterPatch({
        summary: "all",
        searchDraft: "",
        filterDraft: {
            ...filterDraftFromUrl(url),
            orgUnitIds: "org-1,org-2",
            includeDescendants: true,
        },
    })
    expect(applied).toMatchObject({
        page: 1,
        orgUnitIds: "org-1,org-2",
        includeDescendants: true,
    })
    const built = buildSalesOrdersSearchParams({
        ...url,
        orgUnitIds: "org-1",
        includeDescendants: true,
        page: 1,
    })
    expect(built).toContain("orgUnitIds=org-1")
    expect(built).toContain("includeDescendants=1")
})

it("客户、合同、采购人员条件序列化和分页保持稳定身份", () => {
    const contract = contractsUrlCodec.parse(
        new URLSearchParams("ownerUserIds=user-2&page=5"),
    )
    expect(
        contractsUrlCodec.parse(
            new URLSearchParams(contractsUrlCodec.build(contract)),
        ),
    ).toEqual(contract)
    const purchase = parsePurchaseOrdersSearchParams(
        new URLSearchParams("ownerUserIds=user-1&page=2"),
    )
    expect(
        parsePurchaseOrdersSearchParams(
            new URLSearchParams(buildPurchaseOrdersSearchParams(purchase)),
        ).ownerUserIds,
    ).toBe("user-1")
    expect(
        writeDirectoryUrl("/sales/customers", {
            scope: "mine",
            status: "active",
            q: "",
            sort: "business",
            dir: "desc",
            page: 2,
            ownerUserIds: "user-2",
            orgUnitIds: "org-1",
        }),
    ).toBe("/sales/customers?ownerUserIds=user-2&orgUnitIds=org-1&page=2")
})
