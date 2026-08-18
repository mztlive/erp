import { describe, expect, it } from "vitest"

import {
    buildWorkspaceSearchParams,
    filterSummaryFor,
    metricKeyFromUrlState,
    parseWorkspaceSearchParams,
    pickLegalWorkspaceQuery,
    toTodayWorkspaceQuery,
    urlStateFromMetricKey,
} from "./url-state"

describe("parseWorkspaceSearchParams", () => {
    it("defaults to inbox without a team scope switch", () => {
        expect(parseWorkspaceSearchParams(new URLSearchParams())).toMatchObject(
            {
                view: "inbox",
                sort: "priority_due",
            },
        )
        expect(
            parseWorkspaceSearchParams(new URLSearchParams("scope=team")),
        ).toMatchObject({ view: "inbox" })
    })

    it("parses legal workbench filters", () => {
        const state = parseWorkspaceSearchParams(
            new URLSearchParams(
                "view=started&due=overdue&family=approval&q=SO-1&sort=due_asc",
            ),
        )
        expect(state.view).toBe("started")
        expect(state.due).toBe("overdue")
        expect(state.family).toBe("approval")
        expect(state.query).toBe("SO-1")
        expect(state.sort).toBe("due_asc")
    })
})

describe("metric filters do not jump routes", () => {
    it("writes overdue and blocked into the same page query", () => {
        expect(metricKeyFromUrlState({ view: "inbox", blocked: true })).toBe(
            "blocked",
        )
        expect(
            urlStateFromMetricKey("overdue", {
                view: "inbox",
                sort: "priority_due",
            }),
        ).toMatchObject({ view: "inbox", due: "overdue", blocked: false })
        expect(
            urlStateFromMetricKey("started", {
                view: "inbox",
                sort: "priority_due",
            }).view,
        ).toBe("started")
    })
})

describe("filterSummaryFor", () => {
    it("never uses team pending wording", () => {
        expect(filterSummaryFor("inbox")).toBe("待我处理")
        expect(filterSummaryFor("overdue")).toBe("已超期")
        expect(filterSummaryFor("blocked")).toBe("受阻")
        expect(filterSummaryFor("started")).toBe("我发起的审批")
    })
})

describe("toTodayWorkspaceQuery", () => {
    it("keeps timezone and omits team scope", () => {
        expect(
            toTodayWorkspaceQuery(
                { view: "inbox", sort: "priority_due", due: "today" },
                "Asia/Shanghai",
            ),
        ).toMatchObject({
            view: "inbox",
            due: "today",
            timezone: "Asia/Shanghai",
        })
    })
})

describe("pickLegalWorkspaceQuery", () => {
    it("maps the retired tasks route onto /workspace and drops team scope", () => {
        expect(pickLegalWorkspaceQuery(new URLSearchParams("scope=team"))).toBe(
            "/workspace",
        )
        expect(
            pickLegalWorkspaceQuery(
                new URLSearchParams("view=approval-blockers&family=finance"),
            ),
        ).toBe("/workspace?blocked=1&family=finance")
        expect(
            buildWorkspaceSearchParams({ view: "inbox", sort: "priority_due" }),
        ).toBe("")
    })
})
