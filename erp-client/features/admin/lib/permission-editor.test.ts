import { describe, expect, it } from "vitest"
import { PERMISSION_GROUPS } from "@/lib/permissions.generated"
import {
    PERMISSION_CATALOG,
    PERMISSION_MATRIX,
    PERMISSION_BY_CODE,
    GROUP_NAME_BY_CODE,
    actionLabel,
    filterMatrixByKeyword,
    permissionGroupSegment,
    selectedItemsByGroup,
    summarizePermissions,
    countSelectedByTab,
} from "./permission-catalog"
import { diffPermissions, filterPermissionView } from "./permission-editor"

describe("角色权限目录", () => {
    it("每个授权编码只展示一次，且保留全部生成权限和关联接口", () => {
        const raw = PERMISSION_GROUPS.flatMap((group) => group.permissions)
        const codes = raw.map(
            (item) => `${item.permission.resource}:${item.permission.action}`,
        )
        const displayed = PERMISSION_CATALOG.flatMap((group) =>
            group.items.map((item) => item.code),
        )
        expect(new Set(displayed).size).toBe(displayed.length)
        expect(new Set(displayed)).toEqual(new Set(codes))
        for (const item of raw) {
            const code = `${item.permission.resource}:${item.permission.action}`
            expect(PERMISSION_BY_CODE.get(code)?.endpoints).toContainEqual({
                method: item.method,
                path: item.path,
            })
        }
        const ids = PERMISSION_CATALOG.map((group) =>
            permissionGroupSegment(group.name),
        )
        expect(new Set(ids).size).toBe(ids.length)
    })
    it("目录、摘要和分类的授权计数使用同一口径", () => {
        const selected = [
            "customer:list",
            "customer:create",
            "sales_order:list",
            "sales_selection_booklet:get",
        ]
        const summary = selectedItemsByGroup(selected)
        for (const group of summary) {
            const matrix = PERMISSION_MATRIX.find(
                (item) => item.name === group.name,
            )!
            expect(
                matrix.codes.filter((code) => selected.includes(code)),
            ).toHaveLength(group.items.length)
        }
        expect(summarizePermissions([...selected, selected[0]!]).total).toBe(
            selected.length,
        )
        const counts = countSelectedByTab(selected)
        expect(counts.business + counts.system).toBe(selected.length)
    })
    it("共享读取权限归业务对象，报表名称仍可搜索", () => {
        expect(GROUP_NAME_BY_CODE.get("customer:list")).toBe("客户")
        expect(GROUP_NAME_BY_CODE.get("sales_order:list")).toBe("销售单")
        expect(
            new Set(
                filterMatrixByKeyword(
                    PERMISSION_MATRIX,
                    "客户经营质量",
                ).flatMap((group) => group.codes),
            ),
        ).toEqual(new Set(["customer:list", "sales_order:list"]))
    })
    it("所有现有动作都有中文文案", () => {
        for (const item of PERMISSION_BY_CODE.values())
            expect(actionLabel(item.action)).toMatch(/[\u4e00-\u9fff]/)
    })
    it("搜索动作后保持行与动作对应，不会错列授权", () => {
        const groups = filterMatrixByKeyword(PERMISSION_MATRIX, "更换链接")
        expect(groups.length).toBeGreaterThan(0)
        for (const group of groups)
            for (const row of group.rows) {
                expect(row.cells).toHaveLength(group.actions.length)
                row.cells.forEach((cell, index) => {
                    if (cell) expect(cell.action).toBe(group.actions[index])
                })
            }
    })
    it("变更视图同时保留新增与已取消权限，并不改变授权数据", () => {
        const initial = ["customer:list", "customer:create"]
        const selected = ["customer:list", "customer:delete"]
        const visible = filterPermissionView(
            PERMISSION_MATRIX,
            "changed",
            selected,
            initial,
        )
        expect(new Set(visible.flatMap((group) => group.codes))).toEqual(
            new Set(["customer:create", "customer:delete"]),
        )
        expect(diffPermissions(selected, initial)).toEqual({
            added: ["customer:delete"],
            removed: ["customer:create"],
        })
        expect(selected).toEqual(["customer:list", "customer:delete"])
        expect(initial).toEqual(["customer:list", "customer:create"])
    })
})
