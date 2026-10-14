import { TriangleAlertIcon } from "lucide-react"

import type { DeliveryCoverageHint as Hint } from "../api/delivery-coverage"

const HINT_TEXT: Record<Exclude<Hint, "NONE">, string> = {
    NO_SUPPLY: "该地址暂无可配送供给，提交后由采购确认能否履约",
    BULK_FREIGHT_EXTRA_ONLY: "该地址仅支持集采发货，需另付运费",
}

/** 收货地区的配送提示；不阻断保存与提交。 */
export function DeliveryCoverageHint({
    hint,
    id,
}: {
    hint: Hint
    id?: string
}) {
    if (hint === "NONE") return null
    return (
        <p
            id={id}
            className="flex items-start gap-1 text-xs text-warning-soft-foreground"
            role="status"
        >
            <TriangleAlertIcon
                className="mt-0.5 size-3.5 shrink-0"
                aria-hidden="true"
            />
            {HINT_TEXT[hint]}
        </p>
    )
}
