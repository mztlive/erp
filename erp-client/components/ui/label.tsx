"use client"

import * as React from "react"

import { cn } from "@/lib/utils"

function Label({ className, ...props }: React.ComponentProps<"label">) {
    return (
        // Association is supplied by each caller through htmlFor or wrapped content.
        // oxlint-disable-next-line jsx-a11y/label-has-associated-control
        <label
            data-slot="label"
            className={cn(
                "flex items-center gap-1.5 text-xs font-medium text-foreground/75 select-none group-data-[disabled=true]:pointer-events-none group-data-[disabled=true]:opacity-50 peer-disabled:cursor-not-allowed peer-disabled:opacity-50 [&>[data-slot=required-mark]]:-ml-1",
                className,
            )}
            {...props}
        />
    )
}

export { Label }
