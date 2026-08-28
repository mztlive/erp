import { describe, expect, test } from "vitest"

import { decimalProgressPercent } from "./decimal-progress"

describe("decimalProgressPercent", () => {
    test("整除得到整数百分比", () => {
        expect(decimalProgressPercent("8.00", "20.00")).toBe(40)
        expect(decimalProgressPercent("20.00", "20.00")).toBe(100)
        expect(decimalProgressPercent("0.00", "20.00")).toBe(0)
    })

    test("超额分配封顶 100", () => {
        expect(decimalProgressPercent("25.00", "20.00")).toBe(100)
    })

    test("总额为零或非法输入为 0", () => {
        expect(decimalProgressPercent("8.00", "0.00")).toBe(0)
        expect(decimalProgressPercent("8.00", "abc")).toBe(0)
        expect(decimalProgressPercent("-1.00", "20.00")).toBe(0)
    })
})
