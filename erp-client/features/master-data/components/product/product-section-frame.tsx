"use client"

import * as React from "react"

import { detailWorkspaceStyles } from "@/components/business/detail-workspace"

function ProductSectionFrame({
    id,
    title,
    showHeading = true,
    description,
    disabled,
    extra,
    children,
}: {
    id?: string
    title: string
    showHeading?: boolean
    description?: React.ReactNode
    disabled?: boolean
    extra?: React.ReactNode
    children: React.ReactNode
}) {
    return (
        <fieldset id={id} disabled={disabled} className="min-w-0 space-y-4">
            <legend className="sr-only">{title}</legend>
            {showHeading || extra ? (
                <div className="flex flex-wrap items-center justify-between gap-3">
                    {showHeading ? (
                        <div className="flex min-w-0 flex-wrap items-baseline gap-x-2.5 gap-y-1">
                            <h2 className={detailWorkspaceStyles.sectionTitle}>
                                {title}
                            </h2>
                            {description ? (
                                <p className="text-xs text-muted-foreground">
                                    {description}
                                </p>
                            ) : null}
                        </div>
                    ) : null}
                    {extra}
                </div>
            ) : null}
            {children}
        </fieldset>
    )
}

export { ProductSectionFrame }
