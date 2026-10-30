import { compareDecimal, subtractFixed } from "@/lib/fixed-decimal"
import type { DepositReconciliation, DepositReservation } from "../types"

const moneyOptions = { maxScale: 2, outputScale: 2 } as const

/** 仅展示原占用尚未消耗、尚未释放的金额，实际释放额由服务端核定。 */
export function reservationRemainder(reservation: DepositReservation) {
    return subtractFixed(
        subtractFixed(
            reservation.amount,
            reservation.consumed_amount,
            moneyOptions,
        ),
        reservation.released_amount,
        moneyOptions,
    )
}

/** 平衡对账仅作为核实候选，不能代替服务端的资金和订单风险校验。 */
export function balancedReconciliations(
    records: readonly DepositReconciliation[],
    original?: DepositReconciliation,
) {
    return records.filter(
        (record) =>
            record.unresolved_transaction_ids.length === 0 &&
            compareDecimal(
                record.remote_balance,
                record.local_balance_at_cutoff,
                2,
            ) === 0 &&
            (!original ||
                (record.covered_from <= original.covered_from &&
                    record.covered_until >= original.covered_until &&
                    record.observed_at >= original.observed_at)),
    )
}
