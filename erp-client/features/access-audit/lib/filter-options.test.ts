import { describe, expect, it } from "vitest"

import {
    ACTION_FILTER_OPTIONS,
    actionFilterLabel,
    auditDateRangeError,
    parseActionFilter,
    parseResultFilter,
    parseRiskFilter,
    parseStatusFilter,
    RESULT_FILTER_RADIO_OPTIONS,
    RISK_FILTER_RADIO_OPTIONS,
    resultFilterLabel,
    riskFilterLabel,
    STATUS_FILTER_RADIO_OPTIONS,
    statusFilterLabel,
} from "./filter-options"

describe("filter option parsers", () => {
    it("parses declared enum values from the URL", () => {
        expect(parseStatusFilter("enabled")).toBe("enabled")
        expect(parseStatusFilter("disabled")).toBe("disabled")
        expect(parseRiskFilter("HIGH_PRIVILEGE")).toBe("HIGH_PRIVILEGE")
        expect(parseResultFilter("UNKNOWN")).toBe("UNKNOWN")
        expect(parseActionFilter("QUERY_AUDIT")).toBe("QUERY_AUDIT")
    })

    it("degrades missing or invalid enum values to the default (all)", () => {
        expect(parseStatusFilter(null)).toBe("all")
        expect(parseStatusFilter("weird")).toBe("all")
        expect(parseRiskFilter("not-a-risk")).toBe("all")
        expect(parseResultFilter("PENDING")).toBe("all")
        expect(parseActionFilter("DELETE_EVERYTHING")).toBe("all")
    })

    it("keeps the fixed option lists within the radio limit including 全部", () => {
        expect(STATUS_FILTER_RADIO_OPTIONS.map((o) => o.label)).toEqual([
            "全部",
            "启用",
            "停用",
        ])
        expect(RISK_FILTER_RADIO_OPTIONS).toHaveLength(5)
        expect(RESULT_FILTER_RADIO_OPTIONS).toHaveLength(5)
        expect(ACTION_FILTER_OPTIONS[0]).toEqual({
            value: "all",
            label: "全部动作",
        })
    })

    it("maps filter codes to business labels for chips", () => {
        expect(statusFilterLabel("enabled")).toBe("启用")
        expect(riskFilterLabel("EMPTY_SCOPE")).toBe("空数据范围")
        expect(resultFilterLabel("DENIED")).toBe("拒绝")
        expect(actionFilterLabel("EMERGENCY_REVOKE_USER_ROLE")).toBe("紧急撤权")
    })
})

describe("auditDateRangeError", () => {
    it("rejects a range where the end precedes the start", () => {
        expect(auditDateRangeError("2026-01-02", "2026-01-01")).toBe(
            "截止日期不能早于起始日期",
        )
    })

    it("accepts empty ends, equal dates and valid ranges", () => {
        expect(auditDateRangeError("", "")).toBeNull()
        expect(auditDateRangeError("2026-01-01", "")).toBeNull()
        expect(auditDateRangeError("2026-01-01", "2026-01-01")).toBeNull()
        expect(auditDateRangeError("2026-01-01", "2026-01-02")).toBeNull()
    })
})
