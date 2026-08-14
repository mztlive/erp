"use client"

import type { ReactNode } from "react"

export function SectionTitle({ children }: { children: ReactNode }) {
    return (
        <h3 className="text-xs font-semibold tracking-wide text-foreground">
            {children}
        </h3>
    )
}
