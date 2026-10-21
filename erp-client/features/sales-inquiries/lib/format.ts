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
