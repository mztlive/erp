import { describe, expect, it } from "vitest"

import {
    BUSINESS_GROUPS,
    PERMISSION_CATALOG,
    PERMISSION_PANEL_TAB_LABEL,
    SYSTEM_GROUPS,
    countSelectedByTab,
    filterGroupsByKeyword,
    isSystemGroup,
    matchesKeyword,
    type PermissionGroupOption,
} from "./permission-catalog"

describe("permission catalog", () => {
    it("partitions every group into business or system without loss", () => {
        expect(PERMISSION_CATALOG.length).toBeGreaterThan(0)
        expect(BUSINESS_GROUPS.length).toBeGreaterThan(0)
        expect(SYSTEM_GROUPS.length).toBeGreaterThan(0)
        expect(BUSINESS_GROUPS.length + SYSTEM_GROUPS.length).toBe(
            PERMISSION_CATALOG.length,
        )
        for (const group of BUSINESS_GROUPS) {
            expect(isSystemGroup(group.name)).toBe(false)
        }
        for (const group of SYSTEM_GROUPS) {
            expect(isSystemGroup(group.name)).toBe(true)
        }
    })

    it("labels both tabs", () => {
        expect(PERMISSION_PANEL_TAB_LABEL.business).toBe("业务")
        expect(PERMISSION_PANEL_TAB_LABEL.system).toBe("系统")
    })

    it("matchesKeyword searches code, description and path case-insensitively", () => {
        const item = {
            code: "admin:list",
            description: "查询管理员列表",
            method: "GET",
            path: "/admin/admins",
        }
        expect(matchesKeyword(item, "admin")).toBe(true)
        expect(matchesKeyword(item, "查询")).toBe(true)
        expect(matchesKeyword(item, "/admin/admins")).toBe(true)
        // q 由调用方先统一小写，这里按契约传小写查询串
        expect(matchesKeyword(item, "admin:list")).toBe(true)
        expect(matchesKeyword(item, "nope")).toBe(false)
    })

    it("filterGroupsByKeyword keeps only groups with matching items", () => {
        const groups: PermissionGroupOption[] = [
            {
                name: "组A",
                description: "A",
                items: [
                    {
                        code: "order:list",
                        description: "订单列表",
                        method: "GET",
                        path: "/orders",
                    },
                ],
            },
            {
                name: "组B",
                description: "B",
                items: [
                    {
                        code: "admin:list",
                        description: "管理员列表",
                        method: "GET",
                        path: "/admins",
                    },
                ],
            },
        ]

        expect(filterGroupsByKeyword(groups, "order")).toEqual([
            {
                name: "组A",
                description: "A",
                items: [groups[0].items[0]],
            },
        ])
        expect(filterGroupsByKeyword(groups, "")).toBe(groups)
        expect(filterGroupsByKeyword(groups, "不存在")).toEqual([])
    })

    it("countSelectedByTab counts per dimension and ignores unknown codes", () => {
        const businessCode = BUSINESS_GROUPS[0]?.items[0]?.code
        const systemCode = SYSTEM_GROUPS[0]?.items[0]?.code
        expect(businessCode).toBeDefined()
        expect(systemCode).toBeDefined()

        expect(countSelectedByTab([])).toEqual({ business: 0, system: 0 })
        expect(
            countSelectedByTab([
                businessCode!,
                systemCode!,
                "ghost:view",
            ]),
        ).toEqual({ business: 1, system: 1 })
    })
})
