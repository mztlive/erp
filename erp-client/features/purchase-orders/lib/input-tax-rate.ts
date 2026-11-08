import { multiplyFixed } from "@/lib/fixed-decimal"

/**
 * 进项税率小数转为百分数，供纸质单和拆单预览共用同一舍入。
 *
 * @param inputTaxRate 进项税率小数，例如 `0.13`。
 * @returns 两位小数的百分数。
 */
export function inputTaxRatePercent(inputTaxRate: string): string {
    return multiplyFixed(inputTaxRate, "100", {
        leftMaxScale: 6,
        rightMaxScale: 0,
        outputScale: 2,
    })
}
