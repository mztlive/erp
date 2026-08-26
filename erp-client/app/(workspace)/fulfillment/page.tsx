import { redirect } from "next/navigation"

/**
 * W09 旧入口兼容层。履约正式执行只允许在 W01 当前任务作业面完成。
 */
export default async function Page({
    searchParams,
}: {
    searchParams: Promise<Record<string, string | string[] | undefined>>
}) {
    const legacy = await searchParams
    const first = (key: string) => {
        const value = legacy[key]
        return Array.isArray(value) ? value[0] : value
    }
    const query =
        first("currentOperationId") ??
        first("purchaseOrderId") ??
        first("warehouseId") ??
        first("sourceDocId")
    const next = new URLSearchParams({
        family: "fulfillment",
        type: "FULFILLMENT_OPERATION",
    })
    if (query?.trim()) next.set("q", query.trim())
    redirect(`/workspace?${next.toString()}`)
}
