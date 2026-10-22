"use client"

import { Checkbox } from "@/components/ui/checkbox"
import { cn } from "@/lib/utils"

/** 商品参与本次提交的复选框，与列表勾选共用表单状态。 */
export function SourcingParticipationControl({
    idPrefix,
    itemName,
    included,
    onChange,
    showHint = true,
}: {
    idPrefix: string
    itemName: string
    included: boolean
    onChange: (included: boolean) => void
    showHint?: boolean
}) {
    const id = `${idPrefix}-participation`
    const hint = showHint && !included
    return (
        <div className="space-y-1">
            <label
                htmlFor={id}
                className="inline-flex cursor-pointer items-center gap-2"
            >
                <Checkbox
                    id={id}
                    checked={included}
                    onCheckedChange={onChange}
                    aria-label={`本次分配 ${itemName}`}
                    aria-describedby={hint ? `${id}-hint` : undefined}
                />
                <span
                    className={cn(
                        "whitespace-nowrap text-xs font-medium",
                        included ? "text-foreground" : "text-muted-foreground",
                    )}
                >
                    {included ? "本次分配" : "暂不分配"}
                </span>
            </label>
            {hint ? (
                <p id={`${id}-hint`} className="text-xs text-muted-foreground">
                    本次不提交，方案已保留
                </p>
            ) : null}
        </div>
    )
}
