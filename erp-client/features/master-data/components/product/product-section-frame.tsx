"use client"

import * as React from "react"

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
        <fieldset id={id} disabled={disabled} className="space-y-5">
            <legend className="sr-only">{title}</legend>
            {showHeading || extra ? (
                <div
                    className={
                        showHeading
                            ? "flex flex-wrap items-start justify-between gap-3 border-b border-grid pb-3"
                            : "flex flex-wrap items-center justify-between gap-3"
                    }
                >
                    {showHeading ? (
                        <div className="min-w-0 space-y-1">
                            <div className="text-base font-semibold tracking-tight">
                                {title}
                            </div>
                            {description ? (
                                <p className="max-w-3xl text-sm leading-5 text-muted-foreground">
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
