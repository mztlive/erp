import { expect, it } from "vitest"

import {
    buildDataScopeSearchParams,
    buildOrganizationSearchParams,
    mergeDataScopeSearchParams,
    mergeOrganizationSearchParams,
    parseDataScopeSearchParams,
    parseOrganizationSearchParams,
} from "./url-state"

it("组织筛选进入 URL，刷新与清除使用同一组键", () => {
    const url = parseOrganizationSearchParams(
        new URLSearchParams("unitId=org-1&q=销售&kind=team&status=enabled"),
    )
    expect(url).toEqual({
        unitId: "org-1",
        q: "销售",
        kind: "team",
        status: "enabled",
    })
    const built = buildOrganizationSearchParams(url)
    expect(built).toContain("unitId=org-1")
    expect(built).toContain("kind=team")
    const cleared = mergeOrganizationSearchParams(new URLSearchParams(built), {
        kind: "all",
        status: "all",
        q: undefined,
        unitId: undefined,
    })
    expect(cleared).toBe("")
})

it("范围配置按资源动作进入 URL 与查询草稿", () => {
    const url = parseDataScopeSearchParams(
        new URLSearchParams(
            "resource=sales_order&action=list&subjectType=role&subjectId=role-sales&scopeType=company",
        ),
    )
    expect(url.resource).toBe("sales_order")
    expect(url.action).toBe("list")
    expect(url.subjectType).toBe("role")
    expect(url.subjectId).toBe("role-sales")
    expect(url.scopeType).toBe("company")
    const built = buildDataScopeSearchParams(url)
    expect(built).toContain("resource=sales_order")
    expect(built).toContain("action=list")
    const merged = mergeDataScopeSearchParams(
        new URLSearchParams("keep=1&resource=old"),
        url,
    )
    expect(merged).toContain("keep=1")
    expect(merged).toContain("resource=sales_order")
    expect(merged).not.toContain("resource=old")
})
