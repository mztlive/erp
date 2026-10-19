"use client"

import { Checkbox } from "@/components/ui/checkbox"
import { Switch } from "@/components/ui/switch"
import { cn } from "@/lib/utils"

/** 商品参与本次提交的显式开关，与列表批量勾选独立。 */
export function SourcingParticipationControl({
    idPrefix,
    itemName,
    included,
    onChange,
    showHint = true,
    control = "switch",
}: {
    idPrefix: string
    itemName: string
    included: boolean
    onChange: (included: boolean) => void
    showHint?: boolean
    /** 弹层等紧凑场景用复选框，列表行保留开关。 */
    control?: "switch" | "checkbox"
}) {
    const id = `${idPrefix}-participation`
    const hint = showHint && !included
    return (
        <div className="space-y-1">
            <label
                htmlFor={id}
                className={cn(
                    "inline-flex cursor-pointer items-center",
                    control === "checkbox" ? "gap-2" : "min-h-8 gap-3",
                )}
            >
                {control === "checkbox" ? (
                    <Checkbox
                        id={id}
                        checked={included}
                        onCheckedChange={onChange}
                        aria-label={`本次分配 ${itemName}`}
                        aria-describedby={hint ? `${id}-hint` : undefined}
                    />
                ) : (
                    <Switch
                        id={id}
                        checked={included}
                        onCheckedChange={onChange}
                        aria-label={`本次分配 ${itemName}`}
                        aria-describedby={hint ? `${id}-hint` : undefined}
                    />
                )}
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
