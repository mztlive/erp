"use client"

import { Switch } from "@/components/ui/switch"
import { cn } from "@/lib/utils"

/** 商品参与本次提交的显式开关，与列表批量勾选独立。 */
export function SourcingParticipationControl({
    idPrefix,
    itemName,
    included,
    onChange,
}: {
    idPrefix: string
    itemName: string
    included: boolean
    onChange: (included: boolean) => void
}) {
    const id = `${idPrefix}-participation`
    return (
        <div className="space-y-1">
            <label
                htmlFor={id}
                className="inline-flex min-h-8 cursor-pointer items-center gap-3"
            >
                <Switch
                    id={id}
                    checked={included}
                    onCheckedChange={onChange}
                    aria-label={`本次分配 ${itemName}`}
                    aria-describedby={!included ? `${id}-hint` : undefined}
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
            {!included ? (
                <p id={`${id}-hint`} className="text-xs text-muted-foreground">
                    本次不提交，方案已保留
                </p>
            ) : null}
        </div>
    )
}
