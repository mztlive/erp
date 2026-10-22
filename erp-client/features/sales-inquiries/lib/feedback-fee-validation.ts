import { compareDecimal } from "@/lib/fixed-decimal"
import type { InquiryFeedbackData, InquiryFee } from "../types"

type FeeField = "name" | "amount_gross" | "tax_rate"
type FeeIssue = { index: number; field: FeeField; message: string }

/** 保存费用前校验必填资料；商品未完成报价及费用有效期仍允许保留为草稿。 */
export function feedbackFeeIssues(fees: InquiryFee[]): FeeIssue[] {
    const issues: FeeIssue[] = []
    fees.forEach((fee, index) => {
        const add = (field: FeeField, message: string) =>
            issues.push({ index, field, message })
        if (!fee.name.trim()) add("name", "请填写费用名称，或移除这项费用")
        else if (fee.name.trim().length > 256)
            add("name", "费用名称不能超过 256 个字符")
        try {
            if (compareDecimal(fee.amount_gross, "0", 2) < 0)
                add("amount_gross", "含税金额不能小于零")
        } catch {
            add("amount_gross", "请填写有效含税金额，最多保留 2 位小数")
        }
        try {
            if (
                compareDecimal(fee.tax_rate, "0", 6) < 0 ||
                compareDecimal(fee.tax_rate, "1", 6) > 0
            )
                add("tax_rate", "税率须在 0 至 1 之间")
        } catch {
            add("tax_rate", "请填写有效税率，最多保留 6 位小数")
        }
    })
    return issues
}

export function validateFeedbackFees(value: InquiryFeedbackData) {
    const issues = feedbackFeeIssues(value.fees)
    return issues.length
        ? {
              fields: Object.fromEntries(
                  issues.map(({ index, field, message }) => [
                      `fees[${index}].${field}`,
                      message,
                  ]),
              ),
          }
        : undefined
}
