import { expect, test } from "vitest"
import { accountPermissionGroups } from "./account-permission-preview"
import type { AdminAccount, AdminRole } from "../types"

const account: AdminAccount = {
    id: "u1",
    name: "张三",
    account: "zhangsan",
    role_ids: ["r1", "r2"],
    created_at: 0,
}
const roles: AdminRole[] = [
    {
        id: "r1",
        name: "销售",
        permissions: ["customer:list", "customer:list"],
        created_at: 0,
    },
    {
        id: "r2",
        name: "经理",
        permissions: ["customer:list", "customer:create"],
        created_at: 0,
    },
    {
        id: "r3",
        name: "无关角色",
        permissions: ["admin:delete"],
        created_at: 0,
    },
]

test("仅展示绑定角色，重复权限保留所有来源但只计一项", () => {
    const preview = accountPermissionGroups(account, roles)
    expect(preview.permissionCount).toBe(2)
    expect(preview.assigned.map((role) => role.id)).toEqual(["r1", "r2"])
    const item = preview.groups
        .flatMap((group) => group.items)
        .find((item) => item.code === "customer:list")
    expect(item?.sources.map((source) => source.name)).toEqual(["销售", "经理"])
    expect(roles[0].permissions).toHaveLength(2)
})

test("全权角色单独标识，未返回的角色不能伪装成完整结果", () => {
    const preview = accountPermissionGroups(account, [
        { ...roles[0], permissions: ["*:*"] },
    ])
    expect(preview.allPermissions).toBe(true)
    expect(preview.groups[0].name).toBe("全部模块")
    expect(preview.missingRoleCount).toBe(1)
})
