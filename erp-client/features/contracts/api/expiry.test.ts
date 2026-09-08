import { afterEach, expect, it, vi } from "vitest"
import { isExpiringWithin30Days } from "./helpers"
afterEach(() => vi.useRealTimers())
it("将到期按上海业务自然日计算，包含当天和第 30 天", () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date("2026-09-07T17:00:00Z"))
    expect(isExpiringWithin30Days("EFFECTIVE", "2026-09-08")).toBe(true)
    expect(isExpiringWithin30Days("EFFECTIVE", "2026-10-08")).toBe(true)
    expect(isExpiringWithin30Days("EFFECTIVE", "2026-09-07")).toBe(false)
    expect(isExpiringWithin30Days("EFFECTIVE", "2026-10-09")).toBe(false)
    expect(isExpiringWithin30Days("TERMINATED", "2026-09-08")).toBe(false)
    expect(isExpiringWithin30Days("EFFECTIVE", null)).toBe(false)
})
