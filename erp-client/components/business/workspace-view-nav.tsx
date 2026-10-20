"use client"

import Link from "next/link"
import type { ReactNode } from "react"
import { cn } from "@/lib/utils"

export function WorkspaceViewNav({ children }: { children: ReactNode }) {
    return (
        <nav
            aria-label="工作视图"
            className="flex flex-wrap items-center gap-x-8 gap-y-1"
        >
            {children}
        </nav>
    )
}

export function WorkspaceViewNavItem({
    id,
    active = false,
    href,
    onClick,
    children,
    "aria-label": ariaLabel,
}: {
    id: string
    active?: boolean
    href?: string
    onClick?: () => void
    children: ReactNode
    "aria-label"?: string
}) {
    const className = cn(
        "relative inline-flex h-8 items-center rounded-sm px-1.5 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
        active
            ? "font-medium text-foreground after:absolute after:inset-x-1.5 after:bottom-0 after:h-0.5 after:rounded-full after:bg-foreground"
            : "text-muted-foreground hover:text-foreground",
    )
    return href ? (
        <Link
            id={id}
            href={href}
            className={className}
            aria-current={active ? "page" : undefined}
            aria-label={ariaLabel}
        >
            {children}
        </Link>
    ) : (
        <button
            id={id}
            type="button"
            className={className}
            aria-pressed={active}
            aria-label={ariaLabel}
            onClick={onClick}
        >
            {children}
        </button>
    )
}
