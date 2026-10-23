import { apiGet } from "@/lib/api"
import { mapWithConcurrency } from "@/features/master-data/lib/sellable-excel-rows"
import { fetchInquiry, inquiryPath } from "../api"
import { allOfferings } from "./supplier-quote-export"
import { systemOfferingQuotes } from "../lib/system-offering-quotes"
import type { InternalQuote } from "../lib/internal-quotes"

export async function fetchInternalQuotes(inquiryId: string, taskId: string) {
    const replies = await apiGet<InternalQuote[]>(
        `${inquiryPath(true, inquiryId)}/internal-quotes`,
        { task_id: taskId },
    )
    const inquiry = await fetchInquiry(inquiryId, true, taskId)
    if (!("work_items" in inquiry))
        throw new Error("请从采购询价任务查看供应商报价")
    const task = inquiry.work_items.find(
        (item) => item.id === taskId && item.status === "OPEN",
    )
    if (!task) throw new Error("当前询价任务已变化，请刷新采购询价列表")
    const lines = inquiry.draft.lines.filter((line) =>
        task.line_ids.includes(line.line_id),
    )
    const offerings = await mapWithConcurrency(
        [...new Set(lines.map((line) => line.sku_id))],
        4,
        allOfferings,
    )
    // 系统参考价随供给更新；已登记回复与已发布反馈仍读取不可变历史。
    return [
        ...systemOfferingQuotes(inquiry, lines, offerings.flat()),
        ...replies,
    ]
}
