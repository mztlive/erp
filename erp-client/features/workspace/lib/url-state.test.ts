import { expect, test } from "vitest"

import {
    filterSummaryFor,
    metricKeyFromUrlState,
    parseWorkspaceSearchParams,
    urlStateFromMetricKey,
} from "./url-state"

test("采购任务族可以从工作台 URL 恢复", () => {
    const state = parseWorkspaceSearchParams(
        new URLSearchParams("family=procurement"),
    )

    expect(state.family).toBe("procurement")
})

test("范围内待办从 URL view=managed 恢复", () => {
    const state = parseWorkspaceSearchParams(
        new URLSearchParams("view=managed"),
    )

    expect(state.view).toBe("managed")
    expect(metricKeyFromUrlState(state)).toBe("managed")
    expect(filterSummaryFor("managed")).toBe("范围内待办")
})

test("切换到范围内待办会清掉超期与受阻筛选", () => {
    const next = urlStateFromMetricKey("managed", {
        view: "inbox",
        due: "overdue",
        blocked: true,
        sort: "priority_due",
        currentWorkItemId: "wi-1",
    })

    expect(next).toMatchObject({
        view: "managed",
        due: undefined,
        blocked: false,
        currentWorkItemId: undefined,
    })
})
