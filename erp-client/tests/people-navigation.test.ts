import { expect, it } from "vitest"
import { isNavItemActive } from "@/lib/nav-active"
import {
    filterNavGroupsByPermissions,
    WORKSPACE_NAV_GROUPS,
} from "@/lib/workspace-registry"

it("组织和角色子视图高亮所属主入口", () => {
    const routes = ["/system/accounts", "/system/access-audit"]
    expect(isNavItemActive("/system/organization", routes[0]!, routes)).toBe(
        true,
    )
    expect(
        isNavItemActive("/system/organization/scopes", routes[1]!, routes),
    ).toBe(true)
    expect(
        isNavItemActive("/system/roles/sales/edit", routes[1]!, routes),
    ).toBe(true)
    expect(
        isNavItemActive("/system/organization/scopes", routes[0]!, routes),
    ).toBe(false)
})
it("仅有组织读取权时主入口直接进入部门视图", () => {
    const groups = filterNavGroupsByPermissions(WORKSPACE_NAV_GROUPS, [
        "org_unit:list",
    ])
    const people = groups
        .flatMap((group) => group.items)
        .find((item) => item.label === "组织与人员")
    expect(people?.href).toBe("/system/organization")
})

it("仅有范围读取权时仍能进入高级维护", () => {
    const groups = filterNavGroupsByPermissions(WORKSPACE_NAV_GROUPS, [
        "data_scope:list",
    ])
    expect(
        groups
            .flatMap((group) => group.items)
            .find((item) => item.label === "角色与权限")?.href,
    ).toBe("/system/organization/scopes")
})
