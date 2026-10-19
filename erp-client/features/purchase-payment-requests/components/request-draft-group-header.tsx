import { CheckCircle2Icon, CircleDashedIcon } from "lucide-react"
import { MoneyValue } from "@/components/business"
import { cn } from "@/lib/utils"
import type { DraftGroup } from "../lib/draft"
import type { PaymentCandidate } from "../types"

export function RequestDraftGroupHeader({
    id,
    group,
    index,
    first,
    active,
    amount,
    incomplete,
    onSelect,
}: {
    id: string
    group: DraftGroup
    index: number
    first?: PaymentCandidate
    active: boolean
    amount: string
    incomplete: boolean
    onSelect: () => void
}) {
    return (
        <button
            id={`${id}-trigger`}
            type="button"
            aria-current={active ? "true" : undefined}
            aria-controls={`${id}-panel`}
            onClick={onSelect}
            className={cn(
                "w-full space-y-1.5 rounded-lg border px-3 py-2 text-left transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none",
                active
                    ? "border-primary/40 bg-background shadow-sm"
                    : "border-transparent hover:bg-background/70",
            )}
        >
            <span className="flex items-start gap-2">
                <span
                    className={cn(
                        "mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-full text-xs",
                        active
                            ? "bg-primary text-primary-foreground"
                            : "bg-muted text-muted-foreground",
                    )}
                >
                    {index + 1}
                </span>
                <span className="min-w-0 break-words text-sm font-medium">
                    {first?.supplier_name ?? "采购来源不可用"}
                </span>
            </span>
            <span className="block break-words text-xs text-muted-foreground">
                付款主体：{first?.our_company_name ?? "请重新选择采购单"}
            </span>
            <span className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs">
                <span
                    className={cn(
                        "inline-flex items-center gap-1",
                        incomplete
                            ? "text-warning-soft-foreground"
                            : "text-success-soft-foreground",
                    )}
                >
                    {incomplete ? (
                        <CircleDashedIcon
                            aria-hidden="true"
                            className="size-3.5"
                        />
                    ) : (
                        <CheckCircle2Icon
                            aria-hidden="true"
                            className="size-3.5"
                        />
                    )}
                    {incomplete ? "待补充" : "已填完整"}
                </span>
                <span className="text-muted-foreground">
                    · {group.ids.length} 张采购单
                </span>
                <span className="ml-auto">
                    <MoneyValue value={amount} />
                </span>
            </span>
        </button>
    )
}
