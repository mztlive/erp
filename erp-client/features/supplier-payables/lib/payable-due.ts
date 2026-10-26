import type {
    PayableRow,
    PayableStatus,
} from "@/features/supplier-payables/types"

/** 沿用应付主分录的到期日，按业务时区自然日展示；已结清不再提示逾期。 */
export function payableDue(
    dueDate: string,
    status: PayableStatus,
): Pick<PayableRow, "dueState" | "dueStateLabel"> {
    if (status === "SETTLED") {
        return { dueState: "settled", dueStateLabel: "已结清" }
    }
    if (!dueDate) {
        return { dueState: "unknown", dueStateLabel: "到期日待确认" }
    }
    const today = new Intl.DateTimeFormat("sv-SE", {
        timeZone: "Asia/Shanghai",
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
    }).format(new Date())
    const days = (Date.parse(today) - Date.parse(dueDate)) / 86_400_000
    if (!Number.isFinite(days)) {
        return { dueState: "unknown", dueStateLabel: "到期日待确认" }
    }
    if (days > 0) {
        return { dueState: "overdue", dueStateLabel: `逾期 ${days} 天` }
    }
    return days === 0
        ? { dueState: "due_today", dueStateLabel: "今日到期" }
        : { dueState: "not_due", dueStateLabel: "未到期" }
}
