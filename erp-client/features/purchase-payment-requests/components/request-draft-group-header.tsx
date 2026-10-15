import { ChevronDownIcon } from "lucide-react"
import { MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { CollapsibleTrigger } from "@/components/ui/collapsible"
import type { DraftForm } from "../hooks/use-draft-form"
import { totalOf, type DraftGroup } from "../lib/draft"
import type { PaymentCandidate } from "../types"

export function RequestDraftGroupHeader({
    id,
    form,
    group,
    index,
    rows,
    open,
}: {
    id: string
    form: DraftForm
    group: DraftGroup
    index: number
    rows: PaymentCandidate[]
    open: boolean
}) {
    const first = rows[0]
    return (
        <CollapsibleTrigger
            id={`${id}-trigger`}
            aria-label={`申请 ${index + 1}，${first.supplier_name}，含 ${rows.length} 张采购单，${open ? "收起" : "展开填写"}`}
            className="grid w-full gap-3 bg-muted p-4 text-left outline-none transition-colors hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring sm:grid-cols-[minmax(0,1fr)_auto]"
        >
            <span className="min-w-0 space-y-2">
                <span className="flex flex-wrap items-center gap-2">
                    <Badge variant="secondary">申请 {index + 1}</Badge>
                    <span className="min-w-0 break-words font-medium">
                        {first.supplier_name}
                    </span>
                    <Badge variant="outline">含 {rows.length} 张采购单</Badge>
                </span>
                <span className="block text-xs text-muted-foreground">
                    付款主体：{first.our_company_name}
                </span>
                {!open && (
                    <span className="block space-y-1 text-sm">
                        {rows.slice(0, 2).map((row) => (
                            <span
                                key={row.payable_account_id}
                                className="block break-words"
                            >
                                <span className="num break-all text-xs text-muted-foreground">
                                    采购单：{row.purchase_order_no}
                                </span>
                                {row.purchase_summary && (
                                    <span> · {row.purchase_summary}</span>
                                )}
                            </span>
                        ))}
                        {rows.length > 2 && (
                            <span className="block text-xs text-muted-foreground">
                                另 {rows.length - 2} 张采购单，展开查看
                            </span>
                        )}
                    </span>
                )}
                <form.Subscribe selector={(state) => state.values}>
                    {(values) => {
                        const current = values.groups.find(
                            (item) => item.key === group.key,
                        )
                        if (!current) return null
                        const missing = [
                            !current.account && "收款账户",
                            !current.date && "希望付款日",
                            !current.description.trim() && "申请说明",
                            group.ids.some(
                                (key) => !values.amounts[key]?.trim(),
                            ) && "申请金额",
                        ].filter(Boolean)
                        return missing.length ? (
                            <span className="block text-xs text-destructive">
                                待补充：{missing.join("、")}
                            </span>
                        ) : null
                    }}
                </form.Subscribe>
            </span>
            <span className="flex items-center justify-between gap-4 sm:justify-end sm:self-start">
                <span className="space-y-1 sm:text-right">
                    <span className="block text-xs text-muted-foreground">
                        本次申请
                    </span>
                    <form.Subscribe selector={(state) => state.values.amounts}>
                        {(amounts) => (
                            <MoneyValue
                                value={totalOf(
                                    group.ids.map((key) => amounts[key] ?? ""),
                                )}
                            />
                        )}
                    </form.Subscribe>
                </span>
                <span className="inline-flex shrink-0 items-center gap-1 text-sm font-medium">
                    {open ? "收起" : "展开填写"}
                    <ChevronDownIcon
                        aria-hidden="true"
                        className={`size-4 transition-transform ${open ? "rotate-180" : ""}`}
                    />
                </span>
            </span>
        </CollapsibleTrigger>
    )
}
