import * as React from "react"

import { cn } from "@/lib/utils"

function Textarea({ className, ...props }: React.ComponentProps<"textarea">) {
    return (
        <textarea
            data-slot="textarea"
            className={cn(
                "flex field-sizing-content min-h-20 w-full resize-none rounded-md border border-input bg-surface-control px-2.5 py-2 text-base text-foreground transition-[background-color,border-color,box-shadow] duration-150 outline-none placeholder:text-muted-foreground/75 hover:border-primary/55 focus-visible:border-primary focus-visible:ring-[3px] focus-visible:ring-ring disabled:cursor-not-allowed disabled:bg-muted disabled:text-muted-foreground disabled:opacity-100 aria-invalid:border-destructive aria-invalid:ring-[3px] aria-invalid:ring-destructive/15 md:text-body-compact dark:aria-invalid:border-destructive/70 dark:aria-invalid:ring-destructive/25",
                className,
            )}
            {...props}
        />
    )
}

export { Textarea }
