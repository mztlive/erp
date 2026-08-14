import { describe, it, expect } from "vitest"

import {
    parseLatency,
    parseMetric,
    parseRecon,
    parseSource,
    w29Href,
} from "./url-state"

describe("parseMetric", () => {
    it.each(["pending_send", "inflight", "timeout", "fail_manual", "acked"])(
        "接受合法值 %s",
        (value) => {
            expect(parseMetric(value)).toBe(value)
        },
    )

    it("非法值或空值回落到 all", () => {
        expect(parseMetric("nope")).toBe("all")
        expect(parseMetric("")).toBe("all")
        expect(parseMetric(null)).toBe("all")
    })
})

describe("parseSource", () => {
    it("接受两种来源枚举", () => {
        expect(parseSource("MIGRATION_BASELINE")).toBe("MIGRATION_BASELINE")
        expect(parseSource("ERP_SALES_REVISION")).toBe("ERP_SALES_REVISION")
    })

    it("非法值回落到 all", () => {
        expect(parseSource("legacy")).toBe("all")
        expect(parseSource(null)).toBe("all")
    })
})

describe("parseLatency", () => {
    it("接受三种时长分组", () => {
        expect(parseLatency("normal")).toBe("normal")
        expect(parseLatency("near_sla")).toBe("near_sla")
        expect(parseLatency("over_sla")).toBe("over_sla")
    })

    it("非法值回落到 all", () => {
        expect(parseLatency("instant")).toBe("all")
        expect(parseLatency(null)).toBe("all")
    })
})

describe("parseRecon", () => {
    it("接受三种对账状态", () => {
        expect(parseRecon("MATCHED")).toBe("MATCHED")
        expect(parseRecon("VERSION_MISMATCH")).toBe("VERSION_MISMATCH")
        expect(parseRecon("NONE")).toBe("NONE")
    })

    it("非法值回落到 all", () => {
        expect(parseRecon("mismatch")).toBe("all")
        expect(parseRecon(null)).toBe("all")
    })
})

describe("w29Href", () => {
    it("同时带错误任务与待办 ID", () => {
        expect(w29Href("wi_1", "et_2")).toBe(
            "/governance/integration-errors?workItemId=wi_1&errorTaskId=et_2&from=W23",
        )
    })

    it("只有待办 ID", () => {
        expect(w29Href("wi_1", undefined)).toBe(
            "/governance/integration-errors?workItemId=wi_1&from=W23",
        )
    })

    it("没有任何 ID 时仍带来源标记", () => {
        expect(w29Href()).toBe("/governance/integration-errors?from=W23")
    })
})
