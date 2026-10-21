"use client"

import * as React from "react"
import { RefreshCwIcon } from "lucide-react"
import { toast } from "@/components/ui/toast"

import { Button } from "@/components/ui/button"
import { getErrorMessage } from "@/lib/api/errors"
import { comboboxText } from "@/lib/ui-text"

export type ComboboxRefreshProps = {
    /** 只刷新候选数据；不得重置表单或修改已选值。失败时必须抛出错误。 */
    onRefresh?: () => Promise<unknown>
}

/** 放在列表外，搜索无结果时仍可刷新；按钮不提交表单。 */
export function ComboboxRefresh({
    id,
    onRefresh,
    loading,
    disabled,
}: ComboboxRefreshProps & {
    id?: string
    loading?: boolean
    disabled?: boolean
}) {
    const [pending, setPending] = React.useState(false)
    const inFlight = React.useRef(false)
    if (!onRefresh) return null

    const refresh = async () => {
        if (inFlight.current || loading || disabled) return
        inFlight.current = true
        setPending(true)
        try {
            await onRefresh()
            toast.add({ title: comboboxText.refreshed, type: "success" })
        } catch (error) {
            toast.add({
                title: comboboxText.refreshFailed,
                description: getErrorMessage(error, comboboxText.refreshFailed),
                type: "error",
            })
        } finally {
            inFlight.current = false
            setPending(false)
        }
    }

    return (
        <div className="shrink-0 border-t p-1">
            <Button
                id={id ? `${id}-refresh` : undefined}
                type="button"
                variant="ghost"
                size="sm"
                className="w-full justify-center text-muted-foreground"
                disabled={disabled || loading || pending}
                aria-busy={pending || undefined}
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => void refresh()}
            >
                <RefreshCwIcon
                    className={pending ? "animate-spin" : undefined}
                />
                {pending ? comboboxText.refreshing : comboboxText.refresh}
            </Button>
        </div>
    )
}
