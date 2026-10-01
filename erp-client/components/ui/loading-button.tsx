"use client"

import type * as React from "react"

import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { cn } from "@/lib/utils"

export type LoadingButtonProps = React.ComponentProps<typeof Button> & {
    loading?: boolean
}

/** 执行期间显示转圈并禁用重复点击，保留 Button 的样式、原生 id 和 ref。 */
export function LoadingButton({
    loading = false,
    disabled,
    children,
    className,
    "aria-busy": ariaBusy,
    ...props
}: LoadingButtonProps) {
    return (
        <Button
            {...props}
            disabled={disabled || loading}
            aria-busy={loading || ariaBusy}
            className={cn(
                loading && "[&>svg:not([data-slot=spinner])]:hidden",
                className,
            )}
        >
            {loading ? (
                <Spinner
                    data-icon="inline-start"
                    role="presentation"
                    aria-label={undefined}
                    aria-hidden="true"
                />
            ) : null}
            {children}
        </Button>
    )
}
