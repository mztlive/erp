import { formatDateTime } from "@/lib/datetime"
import { sumFixed } from "@/lib/fixed-decimal"
import type { PublicPageView, PublicReceiptView } from "../types"

/** 回执与导出共用已提交明细，不读取尚未提交的选择或重新计算金额。 */
export function receiptContent(
    page: PublicPageView,
    receipt: PublicReceiptView,
) {
    const byQuantity = page.submit_mode !== "MALL_REDEEM"
    const displayItems = new Map(page.items.map((item) => [item.item_id, item]))
    const items = receipt.items.map((choice) => {
        const item = displayItems.get(choice.item_id)
        return {
            id: choice.item_id,
            name: item?.name ?? "商品资料暂不可用，请联系销售核对",
            coverPath: item?.cover_path,
            specification: item?.specification ?? [],
            members: item?.members ?? [],
            quantity:
                byQuantity && choice.quantity != null
                    ? String(choice.quantity)
                    : null,
            amount: byQuantity ? choice.line_amount : null,
        }
    })
    const quantities = items.flatMap((item) =>
        item.quantity == null ? [] : [item.quantity],
    )
    return {
        items,
        notices: page.notices,
        byQuantity,
        total: byQuantity ? receipt.total_amount : null,
        quantity:
            byQuantity && quantities.length === items.length && items.length > 0
                ? sumFixed(quantities, { maxScale: 0, outputScale: 0 })
                : null,
        submittedAt: formatDateTime(
            new Date(receipt.submitted_at * 1000).toISOString(),
            "default",
        ),
    }
}

export type ReceiptContent = ReturnType<typeof receiptContent>
