"use client"
import type { ReactNode, Ref } from "react"
import { Button } from "@/components/ui/button"
import {
    Tooltip,
    TooltipTrigger,
    TooltipContent,
} from "@/components/ui/tooltip"

export function InquiryIconAction({
    id,
    label,
    onClick,
    disabled,
    children,
    ref,
}: {
    id: string
    label: string
    onClick: () => void
    disabled?: boolean
    children: ReactNode
    ref?: Ref<HTMLButtonElement>
}) {
    return (
        <Tooltip>
            <TooltipTrigger
                id={id}
                render={
                    <Button
                        ref={ref}
                        id={id}
                        type="button"
                        variant="ghost"
                        size="icon-sm"
                        className="size-8 text-muted-foreground"
                        aria-label={label}
                        disabled={disabled}
                        onClick={onClick}
                    >
                        {children}
                    </Button>
                }
            />
            <TooltipContent>{label}</TooltipContent>
        </Tooltip>
    )
}
