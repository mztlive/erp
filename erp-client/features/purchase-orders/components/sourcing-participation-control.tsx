"use client"

import { Checkbox } from "@/components/ui/checkbox"
import { Switch } from "@/components/ui/switch"
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { cn } from "@/lib/utils"

/** 商品参与本次提交的开关，与列表勾选共用表单状态。 */
export function SourcingParticipationControl({
    idPrefix,
    itemName,
    included,
    onChange,
    showHint = true,
    variant = "inline",
}: {
    idPrefix: string
    itemName: string
    included: boolean
    onChange: (included: boolean) => void
    showHint?: boolean
    variant?: "inline" | "pill"
}) {
    const id = `${idPrefix}-participation`
    const hint = showHint && !included

    if (variant === "pill") {
        return (
            <Tooltip>
                <TooltipTrigger render={<span className="inline-flex" />}>
                    <label
                        htmlFor={id}
                        className={cn(
                            "inline-flex h-control-sm cursor-pointer items-center gap-2 rounded-lg border px-2.5 select-none transition-colors",
                            included
                                ? "border-border/70 bg-card hover:bg-surface-sunken"
                                : "border-dashed border-border bg-surface-sunken hover:bg-muted/70",
                        )}
                    >
                        <Switch
                            id={id}
                            size="sm"
                            checked={included}
                            onCheckedChange={(checked) =>
                                onChange(checked === true)
                            }
                            aria-label={`本次下单 ${itemName}`}
                        />
                        <span
                            className={cn(
                                "whitespace-nowrap text-xs font-medium",
                                included
                                    ? "text-foreground"
                                    : "text-muted-foreground",
                            )}
                        >
                            {included ? "本次下单" : "暂缓下单"}
                        </span>
                    </label>
                </TooltipTrigger>
                <TooltipContent side="bottom" align="end">
                    {included
                        ? "已纳入本次采购提交；关闭后将暂缓处理，已配方案保留在草稿中。"
                        : "当前已暂缓，本次不会提交此商品；开启后重新纳入本次下单。"}
                </TooltipContent>
            </Tooltip>
        )
    }

    return (
        <div className="space-y-1">
            <label
                htmlFor={id}
                className="inline-flex cursor-pointer items-center gap-2 select-none transition-colors"
            >
                <Checkbox
                    id={id}
                    checked={included}
                    onCheckedChange={onChange}
                    aria-label={`本次下单 ${itemName}`}
                    aria-describedby={hint ? `${id}-hint` : undefined}
                />
                <span
                    className={cn(
                        "whitespace-nowrap text-xs font-medium",
                        included ? "text-foreground" : "text-muted-foreground",
                    )}
                >
                    {included ? "本次下单" : "暂缓下单"}
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
