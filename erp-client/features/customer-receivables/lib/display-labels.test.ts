import { describe, expect, test } from "vitest"

import { businessLabelOrPlaceholder } from "./display-labels"

describe("businessLabelOrPlaceholder", () => {
    test("保留并清理真实业务名称", () => {
        expect(
            businessLabelOrPlaceholder(" 华东客户 ", "party-1", "名称待补全"),
        ).toBe("华东客户")
    })

    test("空名称不回退内部 ID", () => {
        expect(
            businessLabelOrPlaceholder(undefined, "party-1", "名称待补全"),
        ).toBe("名称待补全")
    })

    test("伪装成名称的内部 ID 不上屏", () => {
        expect(
            businessLabelOrPlaceholder(" party-1 ", "party-1", "名称待补全"),
        ).toBe("名称待补全")
    })
})
