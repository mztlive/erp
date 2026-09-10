import { subtractFixed, sumFixed, compareDecimal } from "@/lib/fixed-decimal"

/** 按每笔应付实际分配税额计算净额；单目标直接沿用整票税额。 */
export const invoiceTaxAllocations = (
    targets: readonly {
        payableAccountId: string
        amount: string
        taxAmount?: string
    }[],
    totalTax: string,
) => {
    const lines = targets.map((target) => {
        const tax = targets.length === 1 ? totalTax : target.taxAmount
        if (tax == null || !tax.trim())
            throw new Error("请填写每笔核销的分配税额")
        if (
            compareDecimal(tax, "0", 2) < 0 ||
            compareDecimal(tax, target.amount, 2) > 0
        )
            throw new Error("分配税额须介于 0 和该笔分配含税金额之间")
        return {
            payable_account_id: target.payableAccountId,
            allocated_gross_amount: target.amount,
            allocated_tax_amount: tax,
            allocated_net_amount: subtractFixed(target.amount, tax, {
                maxScale: 2,
                outputScale: 2,
            }),
        }
    })
    if (
        compareDecimal(
            sumFixed(
                lines.map((line) => line.allocated_tax_amount),
                { maxScale: 2, outputScale: 2 },
            ),
            totalTax,
            2,
        ) !== 0
    )
        throw new Error("各笔分配税额合计必须等于发票税额")
    return lines
}
