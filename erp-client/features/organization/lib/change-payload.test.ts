import { expect, it } from "vitest"

import {
    buildOrganizationChangeRequest,
    EMPTY_CHANGE_DRAFT,
    shanghaiDateTimeToUnix,
} from "./change-payload"

it("提交命令携带 expected_version 且调岗使用人员 ID", () => {
    const request = buildOrganizationChangeRequest(12, "key-1", {
        ...EMPTY_CHANGE_DRAFT,
        operation: "transfer_member",
        userId: "user-1",
        orgUnitId: "org-2",
        reason: "调入二组",
    })
    expect(request.expected_version).toBe(12)
    expect(request.idempotency_key).toBe("key-1")
    expect(request.change).toEqual({
        operation: "transfer_member",
        user_id: "user-1",
        org_unit_id: "org-2",
    })
})

it("管理授权有效期按上海时区转为服务端时点", () => {
    expect(shanghaiDateTimeToUnix("2026-09-15T18:00:00")).toBe(
        Date.parse("2026-09-15T18:00:00+08:00") / 1000,
    )
    const request = buildOrganizationChangeRequest(1, "k", {
        ...EMPTY_CHANGE_DRAFT,
        operation: "grant_management",
        userId: "user-1",
        roleId: "role-sales",
        orgUnitId: "org-1",
        includeDescendants: "true",
        validTo: "2026-09-15T18:00:00",
        reason: "跨团队管理",
    })
    expect(request.change).toMatchObject({
        operation: "grant_management",
        include_descendants: true,
        valid_to: shanghaiDateTimeToUnix("2026-09-15T18:00:00"),
    })
})
