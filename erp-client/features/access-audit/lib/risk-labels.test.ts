import { describe, expect, it } from "vitest"

import { riskLabel } from "./risk-labels"

describe("riskLabel", () => {
    it("maps known risk flags to business labels", () => {
        expect(riskLabel("HIGH_PRIVILEGE")).toBe("高权限")
        expect(riskLabel("EMPTY_SCOPE")).toBe("空数据范围")
        expect(riskLabel("EXPIRING_SOON")).toBe("即将过期")
        expect(riskLabel("ACCESS_ADMIN")).toBe("权限管理")
        expect(riskLabel("PENDING_DISABLE")).toBe("待停用")
        expect(riskLabel("REVOKED")).toBe("已撤权")
    })

    it("passes unknown flags through unchanged", () => {
        expect(riskLabel("SOMETHING_NEW")).toBe("SOMETHING_NEW")
        expect(riskLabel("")).toBe("")
    })
})
