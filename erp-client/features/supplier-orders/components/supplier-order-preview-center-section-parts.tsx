"use client"

import type { ReactNode } from "react"

import { surfaceInsetClassName } from "@/components/business"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { cn } from "@/lib/utils"

export function Item({ label, value }: { label: string; value: ReactNode }) {
    return (
        <DescriptionItem>
            <DescriptionTerm>{label}</DescriptionTerm>
            <DescriptionDetails>{value}</DescriptionDetails>
        </DescriptionItem>
    )
}

export function FactGap({
    title,
    status,
    amount,
    gap,
}: {
    title: string
    status: string
    amount?: string | null
    gap?: string
}) {
    return (
        <div className={cn(surfaceInsetClassName, "p-3 text-xs")}>
            <div className="font-medium">{title}</div>
            <div className="mt-1">{status}</div>
            {amount != null && amount !== "" ? (
                <div className="mt-1 num text-muted-foreground">{amount}</div>
            ) : null}
            {gap ? (
                <p className="mt-2 text-tiny text-warning-soft-foreground">
                    缺口：{gap}
                </p>
            ) : (
                <p className="mt-2 text-tiny text-muted-foreground">
                    无可见缺口
                </p>
            )}
        </div>
    )
}
