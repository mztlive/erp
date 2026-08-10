"use client"

import { Separator as SeparatorPrimitive } from "@base-ui/react/separator"

import { cn } from "@/lib/utils"

function Separator({
    className,
    orientation = "horizontal",
    ...props
}: SeparatorPrimitive.Props) {
    return (
        <SeparatorPrimitive
            data-slot="separator"
            orientation={orientation}
            className={cn(
                // 分割线用次级描边（grid / colorBorderSecondary），避免与控件边框同重
                "shrink-0 bg-border/40 data-horizontal:h-px data-horizontal:w-full data-vertical:w-px data-vertical:self-stretch",
                className,
            )}
            {...props}
        />
    )
}

export { Separator }
