import { useState } from "react"
import { CheckIcon, CopyIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { PaymentCandidate } from "../types"

export function RequestDraftOrderNumber({
    candidate,
}: {
    candidate: PaymentCandidate
}) {
    const [result, setResult] = useState("")
    return (
        <div className="mt-1 min-w-0">
            <div className="flex min-w-0 items-center gap-1">
                <span
                    className="num min-w-0 truncate text-xs text-muted-foreground"
                    title={candidate.purchase_order_no}
                >
                    {candidate.purchase_order_no}
                </span>
                <Button
                    id={`ppr-copy-order-${toAutomationIdSegment(candidate.payable_account_id)}`}
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    className="shrink-0 text-muted-foreground"
                    aria-label={`复制采购单号 ${candidate.purchase_order_no}`}
                    onClick={async () => {
                        try {
                            await navigator.clipboard.writeText(
                                candidate.purchase_order_no,
                            )
                            setResult("已复制采购单号")
                        } catch {
                            setResult(
                                "复制失败，请展开付款依据查看并复制完整单号",
                            )
                        }
                    }}
                >
                    {result === "已复制采购单号" ? <CheckIcon /> : <CopyIcon />}
                </Button>
            </div>
            <span role="status" className="text-xs text-muted-foreground">
                {result}
            </span>
        </div>
    )
}
