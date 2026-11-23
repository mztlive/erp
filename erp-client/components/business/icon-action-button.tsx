"use client"

import type { ReactNode } from "react"

import { Button, buttonVariants } from "@/components/ui/button"
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip"

/** 标题栏图标动作：可跳转或触发回调，图标本身不承载文字。 */
export function IconActionButton({
    id,
    label,
    testId,
    href,
    disabled,
    onClick,
    children,
}: {
    id?: string
    label: string
    testId: string
    href?: string
    disabled?: boolean
    onClick?: () => void
    children: ReactNode
}) {
    return (
        <Tooltip>
            <TooltipTrigger
                id={id}
                render={
                    href ? (
                        <a
                            id={id}
                            href={href}
                            aria-label={label}
                            data-testid={testId}
                            className={buttonVariants({
                                variant: "ghost",
                                size: "icon-sm",
                            })}
                        />
                    ) : (
                        <Button
                            id={id}
                            type="button"
                            variant="ghost"
                            size="icon-sm"
                            aria-label={label}
                            data-testid={testId}
                            disabled={disabled}
                            onClick={onClick}
                        />
                    )
                }
            >
                {children}
            </TooltipTrigger>
            <TooltipContent>{label}</TooltipContent>
        </Tooltip>
    )
}
