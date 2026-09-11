import { describe, expect, it } from "vitest"

import {
    calcTotalParts,
    calcUploadPercent,
    formatUploadMegabytes,
} from "@/features/master-data/hooks/use-product-import-upload"

describe("calcTotalParts", () => {
    it("rounds up partial chunks and keeps single part for small files", () => {
        expect(calcTotalParts(1, 8 * 1024 * 1024)).toBe(1)
        expect(calcTotalParts(8 * 1024 * 1024, 8 * 1024 * 1024)).toBe(1)
        expect(calcTotalParts(8 * 1024 * 1024 + 1, 8 * 1024 * 1024)).toBe(2)
    })
})

describe("calcUploadPercent", () => {
    it("clamps progress into 0-100", () => {
        expect(calcUploadPercent(0, 100)).toBe(0)
        expect(calcUploadPercent(50, 100)).toBe(50)
        expect(calcUploadPercent(100, 100)).toBe(100)
        expect(calcUploadPercent(150, 100)).toBe(100)
        expect(calcUploadPercent(0, 0)).toBe(0)
    })
})

describe("formatUploadMegabytes", () => {
    it("formats bytes with one decimal place", () => {
        expect(formatUploadMegabytes(0)).toBe("0.0 MB")
        expect(formatUploadMegabytes(1024 * 1024)).toBe("1.0 MB")
    })
})
