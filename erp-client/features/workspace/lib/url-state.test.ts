import { describe, expect, it } from "vitest"

import { sequentialText } from "@/lib/ui-text"
import {
    buildGroupAllHref,
    buildTaskQueueHref,
    buildWorkspaceSearchParams,
    filterSummaryFor,
    metricKeyFromUrlState,
    parseWorkspaceSearchParams,
    toTodayWorkspaceQuery,
    urlStateFromMetricKey,
} from "./url-state"

describe("parseWorkspaceSearchParams", () => {
    it("falls back to the mine scope for an empty query string", () => {
        expect(parseWorkspaceSearchParams(new URLSearchParams())).toEqual({
            scope: "mine",
        })
    })

    it("parses explicit enum values", () => {
        const state = parseWorkspaceSearchParams(
            new URLSearchParams("scope=team&due=today&family=exception"),
        )
        expect(state).toEqual({
            scope: "team",
            due: "today",
            family: "exception",
        })
    })

    it("ignores invalid enum values and falls back to defaults", () => {
        const state = parseWorkspaceSearchParams(
            new URLSearchParams("scope=bogus&due=week"),
        )
        expect(state).toEqual({ scope: "mine" })
    })
})

describe("buildWorkspaceSearchParams", () => {
    it("omits the default scope to keep the url minimal", () => {
        expect(buildWorkspaceSearchParams({ scope: "mine" })).toBe("")
    })

    it("serializes non-default filters in field order", () => {
        expect(
            buildWorkspaceSearchParams({
                scope: "team",
                due: "overdue",
                family: "finance",
            }),
        ).toBe("?scope=team&due=overdue&family=finance")
    })

    it("round-trips parsed state", () => {
        const raw = "scope=team&due=overdue"
        const state = parseWorkspaceSearchParams(new URLSearchParams(raw))
        expect(buildWorkspaceSearchParams(state)).toBe(`?${raw}`)
    })
})

describe("metricKeyFromUrlState", () => {
    it("maps due filters before family and defaults to mine", () => {
        expect(metricKeyFromUrlState({ due: "today" })).toBe("due_today")
        expect(metricKeyFromUrlState({ due: "overdue" })).toBe("overdue")
        expect(metricKeyFromUrlState({ family: "exception" })).toBe("exception")
        expect(metricKeyFromUrlState({ family: "finance" })).toBe("mine")
        expect(metricKeyFromUrlState({})).toBe("mine")
    })
})

describe("urlStateFromMetricKey", () => {
    const current = { scope: "team" as const }

    it("selects due_today and clears the family filter", () => {
        expect(urlStateFromMetricKey("due_today", current)).toEqual({
            scope: "team",
            due: "today",
            family: undefined,
        })
    })

    it("selects overdue and clears the family filter", () => {
        expect(urlStateFromMetricKey("overdue", current)).toEqual({
            scope: "team",
            due: "overdue",
            family: undefined,
        })
    })

    it("selects the exception family and clears the due filter", () => {
        expect(urlStateFromMetricKey("exception", current)).toEqual({
            scope: "team",
            due: undefined,
            family: "exception",
        })
    })

    it("returns to the default filters for mine but keeps the scope", () => {
        expect(urlStateFromMetricKey("mine", current)).toEqual({
            scope: "team",
            due: undefined,
            family: undefined,
        })
    })
})

describe("toTodayWorkspaceQuery", () => {
    it("attaches the timezone to the url state", () => {
        expect(
            toTodayWorkspaceQuery(
                { scope: "team", due: "today" },
                "Asia/Shanghai",
            ),
        ).toEqual({
            scope: "team",
            due: "today",
            family: undefined,
            timezone: "Asia/Shanghai",
        })
    })
})

describe("buildTaskQueueHref", () => {
    it("always carries the scope", () => {
        expect(buildTaskQueueHref({ scope: "mine" })).toBe(
            "/workspace/tasks?scope=mine",
        )
    })

    it("appends active filters", () => {
        expect(
            buildTaskQueueHref({ scope: "team", due: "today" }),
        ).toBe("/workspace/tasks?scope=team&due=today")
        expect(
            buildTaskQueueHref({
                scope: "team",
                due: "today",
                family: "finance",
            }),
        ).toBe("/workspace/tasks?scope=team&due=today&family=finance")
    })
})

describe("buildGroupAllHref", () => {
    it("narrows the task queue link to the group family", () => {
        expect(buildGroupAllHref({ scope: "mine" }, "finance")).toBe(
            "/workspace/tasks?scope=mine&family=finance",
        )
    })
})

describe("filterSummaryFor", () => {
    it("matches the title wording to the metric and scope", () => {
        expect(filterSummaryFor("mine", "mine")).toBe(sequentialText.minePending)
        expect(filterSummaryFor("mine", "team")).toBe(sequentialText.teamPending)
        expect(filterSummaryFor("due_today", "mine")).toBe("今日到期")
        expect(filterSummaryFor("overdue", "mine")).toBe("已超期")
        expect(filterSummaryFor("exception", "team")).toBe("同步异常")
    })
})
