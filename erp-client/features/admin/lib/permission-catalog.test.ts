import { describe, expect, it } from "vitest"

import {
    BUSINESS_GROUPS,
    GROUP_NAME_BY_CODE,
    PERMISSION_BY_CODE,
    PERMISSION_CATALOG,
    PERMISSION_MATRIX,
    PERMISSION_PANEL_TAB_LABEL,
    SYSTEM_GROUPS,
    actionLabel,
    countSelectedByTab,
    filterGroupsByKeyword,
    filterMatrixByKeyword,
    isDangerousAction,
    isSystemGroup,
    matchesKeyword,
    matrixGroupsForTab,
    resourceLabel,
    selectedItemsByGroup,
    summarizePermissions,
    type PermissionGroupOption,
    type PermissionItemOption,
} from "./permission-catalog"

function item(
    overrides: Partial<PermissionItemOption> = {},
): PermissionItemOption {
    return {
        code: "admin:list",
        resource: "admin",
        action: "list",
        description: "查询管理员列表",
        endpoints: [{ method: "GET", path: "/admin/admins" }],
        dangerous: false,
        ...overrides,
    }
}

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

    it("shows every permission code exactly once", () => {
        const codes = PERMISSION_CATALOG.flatMap((group) =>
            group.items.map((entry) => entry.code),
        )
        expect(new Set(codes).size).toBe(codes.length)
        expect(PERMISSION_BY_CODE.size).toBe(codes.length)
        expect(GROUP_NAME_BY_CODE.size).toBe(codes.length)
    })

    it("merges the endpoints of a permission that serves several routes", () => {
        const multi = [...PERMISSION_BY_CODE.values()].find(
            (entry) => entry.endpoints.length > 1,
        )
        expect(multi).toBeDefined()
        expect(multi!.endpoints.length).toBeGreaterThan(1)
    })

    it("translates resources and actions into business wording", () => {
        expect(resourceLabel("admin")).toBe("管理员")
        expect(resourceLabel("work_item")).toBe("待办")
        expect(actionLabel("list")).toBe("查看列表")
        expect(actionLabel("delete")).toBe("删除")
        // 未知动作原样回落，不抛错
        expect(actionLabel("__unknown__")).toBe("__unknown__")
    })

    it("keeps implementation jargon and document codes out of every visible string", () => {
        const banned = ["投影", "事实", "水位", "租约", "幂等", "指纹"]
        const texts = PERMISSION_MATRIX.flatMap((group) => [
            group.name,
            group.description,
            ...group.rows.map((row) => row.label),
        ])
        for (const text of texts) {
            for (const word of banned) {
                expect(text).not.toContain(word)
            }
            // 文档编号（W05 等）只属于设计文档，不进界面
            expect(text).not.toMatch(/W\d\d/)
        }
    })

    it("flags irreversible or sensitive actions", () => {
        expect(isDangerousAction("delete")).toBe(true)
        expect(isDangerousAction("reveal")).toBe(true)
        expect(isDangerousAction("list")).toBe(false)
    })

    it("matchesKeyword searches code, description, labels and paths", () => {
        const entry = item()
        expect(matchesKeyword(entry, "admin")).toBe(true)
        expect(matchesKeyword(entry, "查询")).toBe(true)
        expect(matchesKeyword(entry, "/admin/admins")).toBe(true)
        expect(matchesKeyword(entry, "管理员")).toBe(true)
        expect(matchesKeyword(entry, "查看列表")).toBe(true)
        expect(matchesKeyword(entry, "admin:list")).toBe(true)
        expect(matchesKeyword(entry, "nope")).toBe(false)
    })

    it("filterGroupsByKeyword keeps only groups with matching items", () => {
        const groups: PermissionGroupOption[] = [
            {
                name: "组A",
                description: "A",
                items: [
                    item({
                        code: "order:list",
                        resource: "order",
                        description: "订单列表",
                        endpoints: [{ method: "GET", path: "/orders" }],
                    }),
                ],
            },
            {
                name: "组B",
                description: "B",
                items: [
                    item({
                        description: "管理员列表",
                        endpoints: [{ method: "GET", path: "/admins" }],
                    }),
                ],
            },
        ]

        expect(filterGroupsByKeyword(groups, "订单")).toEqual([
            {
                name: "组A",
                description: "A",
                items: [groups[0]!.items[0]],
            },
        ])
        expect(filterGroupsByKeyword(groups, "")).toBe(groups)
        expect(filterGroupsByKeyword(groups, "不存在")).toEqual([])
    })

    it("builds one matrix row per object and one column per action", () => {
        for (const group of PERMISSION_MATRIX) {
            expect(group.rows.length).toBeGreaterThan(0)
            expect(group.actions.length).toBeGreaterThan(0)
            for (const row of group.rows) {
                expect(row.cells.length).toBe(group.actions.length)
                for (const [index, cell] of row.cells.entries()) {
                    if (cell) expect(cell.action).toBe(group.actions[index])
                }
            }
            // 组内编码 = 各行编码之和，矩阵不丢权限
            expect(new Set(group.codes).size).toBe(group.codes.length)
            expect(group.codes.length).toBe(
                group.rows.reduce((sum, row) => sum + row.codes.length, 0),
            )
        }
    })

    it("splits the matrix by tab without loss", () => {
        expect(
            matrixGroupsForTab("business").length +
                matrixGroupsForTab("system").length,
        ).toBe(PERMISSION_MATRIX.length)
    })

    it("filterMatrixByKeyword narrows to the matching cells", () => {
        const group = PERMISSION_MATRIX[0]!
        const code = group.rows[0]!.codes[0]!
        const filtered = filterMatrixByKeyword([group], code.toLowerCase())

        expect(filtered).toHaveLength(1)
        expect(filtered[0]!.codes).toEqual([code])
        expect(filterMatrixByKeyword([group], "")).toEqual([group])
        expect(filterMatrixByKeyword([group], "__nothing__")).toEqual([])
    })

    it("filterMatrixByKeyword keeps the whole group when its name matches", () => {
        const group = PERMISSION_MATRIX[0]!
        expect(
            filterMatrixByKeyword([group], group.name.toLowerCase()),
        ).toEqual([group])
    })

    it("countSelectedByTab counts per dimension and ignores unknown codes", () => {
        const businessCode = BUSINESS_GROUPS[0]?.items[0]?.code
        const systemCode = SYSTEM_GROUPS[0]?.items[0]?.code
        expect(businessCode).toBeDefined()
        expect(systemCode).toBeDefined()

        expect(countSelectedByTab([])).toEqual({ business: 0, system: 0 })
        expect(
            countSelectedByTab([businessCode!, systemCode!, "ghost:view"]),
        ).toEqual({ business: 1, system: 1 })
    })

    it("summarizePermissions groups counts and reports wildcard / unknown codes", () => {
        const group = PERMISSION_CATALOG[0]!
        const codes = group.items.slice(0, 2).map((entry) => entry.code)

        const summary = summarizePermissions(codes)
        expect(summary.wildcard).toBe(false)
        expect(summary.total).toBe(codes.length)
        expect(summary.unknown).toBe(0)
        expect(summary.groups[0]).toEqual({
            name: group.name,
            count: codes.length,
        })

        const mixed = summarizePermissions([...codes, "*:*", "ghost:view"])
        expect(mixed.wildcard).toBe(true)
        expect(mixed.unknown).toBe(1)
        expect(mixed.total).toBe(codes.length)
    })

    it("selectedItemsByGroup returns catalog order and drops unknown codes", () => {
        const group = PERMISSION_CATALOG[0]!
        const codes = group.items.slice(0, 2).map((entry) => entry.code)

        const grouped = selectedItemsByGroup([...codes, "ghost:view"])
        expect(grouped).toHaveLength(1)
        expect(grouped[0]!.name).toBe(group.name)
        expect(grouped[0]!.items.map((entry) => entry.code)).toEqual(codes)
        expect(selectedItemsByGroup([])).toEqual([])
    })
})
