import type { AllocationLine } from "@/features/customer-receivables/types"
import { sumFixed } from "@/lib/fixed-decimal"

/**
 * 成交含税金额减去已回款，得到本单待回款。
 *
 * @param gross 销售单含税成交金额
 * @param received 已回款含税金额
 * @returns 待回款十进制字符串；非法输入时回退成交金额
 */
export function remainingReceivableAmount(
    gross: string,
    received: string,
): string {
    try {
        return sumFixed([gross, `-${received}`], {
            maxScale: 2,
            outputScale: 2,
            allowNegative: true,
        })
    } catch {
        return gross
    }
}

/**
 * 本单应收子账及其分录的稳定身份，用来判断回款/发票是否核到本单。
 *
 * @param accounts 本单应收子账
 * @returns 子账 ID 与分录 ID 集合
 */
export function receivableTargetIds(
    accounts: readonly {
        accountId: string
        entries: readonly { entryId: string }[]
    }[],
): Set<string> {
    const ids = new Set<string>()
    for (const account of accounts) {
        ids.add(account.accountId)
        for (const entry of account.entries) {
            ids.add(entry.entryId)
        }
    }
    return ids
}

/**
 * 已过账核销里，落到指定应收身份的净金额。
 *
 * @param allocations 回款或发票上的核销明细
 * @param targetIds 本单应收子账/分录 ID
 * @returns 核到本单的含税净额；没有任何命中时为 `0.00`
 */
export function amountAllocatedToTargets(
    allocations: readonly Pick<
        AllocationLine,
        "targetId" | "amountGross" | "action" | "isPosted"
    >[],
    targetIds: ReadonlySet<string>,
): string {
    const parts: string[] = []
    for (const allocation of allocations) {
        if (!allocation.isPosted || !targetIds.has(allocation.targetId)) {
            continue
        }
        parts.push(
            allocation.action === "REVERSE"
                ? `-${allocation.amountGross}`
                : allocation.amountGross,
        )
    }
    if (parts.length === 0) {
        return "0.00"
    }
    return sumFixed(parts, {
        maxScale: 2,
        outputScale: 2,
        allowNegative: true,
    })
}
