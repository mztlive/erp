import { formatDateTime } from "@/lib/datetime"

export function inquiryDate(value: number): string {
    return value
        ? formatDateTime(new Date(value * 1000).toISOString())
        : "未设置"
}

export function inquiryDateInput(value: number): string {
    if (!value) return ""
    const date = new Date(value * 1000)
    return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`
}

export function inquiryDateSeconds(value: string): number {
    if (!value) return 0
    return Math.floor(new Date(`${value}T23:59:59`).getTime() / 1000)
}

export type InquiryDeadlineMeta = {
    label: string
    hint?: string
    tone: "neutral" | "warning" | "destructive"
}

export function inquiryDeadlineMeta(
    deadlineAt: number,
    active: boolean,
): InquiryDeadlineMeta {
    const label = inquiryDateInput(deadlineAt) || "未设置"
    if (!deadlineAt || !active) {
        return { label, tone: "neutral" }
    }
    const target = new Date(deadlineAt * 1000)
    const now = new Date()
    const targetDay = Date.UTC(
        target.getFullYear(),
        target.getMonth(),
        target.getDate(),
    )
    const today = Date.UTC(now.getFullYear(), now.getMonth(), now.getDate())
    const days = Math.round((targetDay - today) / 86_400_000)
    if (days < 0) {
        return {
            label,
            hint: `已超期 ${Math.abs(days)} 天`,
            tone: "destructive",
        }
    }
    if (days === 0) {
        return { label, hint: "今日截止", tone: "warning" }
    }
    if (days <= 3) {
        return { label, hint: `剩余 ${days} 天`, tone: "warning" }
    }
    return { label, hint: `剩余 ${days} 天`, tone: "neutral" }
}
