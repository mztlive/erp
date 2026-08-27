import { expect, test } from "vitest"

import { parseWorkspaceSearchParams } from "./url-state"

test("采购任务族可以从工作台 URL 恢复", () => {
    const state = parseWorkspaceSearchParams(
        new URLSearchParams("family=procurement"),
    )

    expect(state.family).toBe("procurement")
})
