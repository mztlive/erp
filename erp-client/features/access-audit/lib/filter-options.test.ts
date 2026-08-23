import { describe, expect, it } from "vitest"

import {
    actionFilterLabel,
    auditDateRangeError,
    parseActionFilter,
    parseResultFilter,
    RESULT_FILTER_RADIO_OPTIONS,
    resultFilterLabel,
} from "./filter-options"

describe("filter option parsers", () => {
    it("parses declared values from the URL", () => {
        expect(parseResultFilter("UNKNOWN")).toBe("UNKNOWN")
        // 动作取后端 action_type 形状（<对象>.<动作>）
        expect(parseActionFilter("user_role.assign")).toBe("user_role.assign")
        expect(parseActionFilter(" data_scope.create ")).toBe(
            "data_scope.create",
        )
    })

    it("degrades missing or invalid values to the default (all)", () => {
        expect(parseResultFilter(null)).toBe("all")
        expect(parseResultFilter("PENDING")).toBe("all")
        expect(parseActionFilter(null)).toBe("all")
        expect(parseActionFilter("")).toBe("all")
        expect(parseActionFilter("DELETE_EVERYTHING")).toBe("all")
        expect(parseActionFilter("no-dot")).toBe("all")
    })

    it("keeps the fixed result options within the radio limit including 全部", () => {
        expect(RESULT_FILTER_RADIO_OPTIONS).toHaveLength(5)
        expect(RESULT_FILTER_RADIO_OPTIONS[0]).toEqual({
            value: "all",
            label: "全部结果",
        })
    })

    it("maps filter codes to business labels for chips", () => {
        expect(resultFilterLabel("DENIED")).toBe("拒绝")
        expect(actionFilterLabel("user_role.revoke")).toBe("用户角色 · 撤权")
        expect(actionFilterLabel("data_scope.create")).toBe("数据范围 · 新建")
        // 不符合约定的取值原样展示，不猜
        expect(actionFilterLabel("legacy")).toBe("legacy")
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
