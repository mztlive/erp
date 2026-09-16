import { expect, test } from "vitest"

import {
    buildWorkspaceSearchParams,
    filterSummaryFor,
    metricKeyFromUrlState,
    parseWorkspaceSearchParams,
    toTodayWorkspaceQuery,
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

test("处理人与来源订单筛选进入 URL 与列表查询", () => {
    const state = parseWorkspaceSearchParams(
        new URLSearchParams(
            "handlerUserIds=u2%2Cu1%2Cu2&salesOrderIds=so-1&purchaseOrderIds=po-2%2Cpo-1",
        ),
    )

    expect(state.handlerUserIds).toBe("u1,u2")
    expect(state.salesOrderIds).toBe("so-1")
    expect(state.purchaseOrderIds).toBe("po-1,po-2")
    const query = toTodayWorkspaceQuery(state, "Asia/Shanghai")
    expect(query.handlerUserIds).toBe("u1,u2")
    expect(query.salesOrderIds).toBe("so-1")
    expect(query.purchaseOrderIds).toBe("po-1,po-2")
    const rebuilt = new URLSearchParams(
        buildWorkspaceSearchParams(state).slice(1),
    )
    expect(rebuilt.get("handlerUserIds")).toBe("u1,u2")
    expect(rebuilt.get("salesOrderIds")).toBe("so-1")
    expect(rebuilt.get("purchaseOrderIds")).toBe("po-1,po-2")
})
