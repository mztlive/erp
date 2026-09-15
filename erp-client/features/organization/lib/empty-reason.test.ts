import { expect, it } from "vitest"

import { organizationEmptyReason } from "./empty-reason"

it("权限未决时不渲染空态", () => {
    expect(
        organizationEmptyReason({
            permissionPending: true,
            noScope: false,
            filtered: false,
            empty: true,
        }),
    ).toBeNull()
})

it("无权限、无范围、筛选无结果分别呈现", () => {
    expect(
        organizationEmptyReason({
            error: { status: 403, message: "forbidden" },
            noScope: false,
            filtered: false,
            empty: false,
        }),
    ).toBe("NO_MODULE_PERMISSION")
    expect(
        organizationEmptyReason({
            noScope: true,
            filtered: true,
            empty: true,
        }),
    ).toBe("NO_DATA_SCOPE")
    expect(
        organizationEmptyReason({
            noScope: false,
            filtered: true,
            empty: true,
        }),
    ).toBe("FILTER_NO_RESULT")
})
