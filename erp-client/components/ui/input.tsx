import * as React from "react"
import { Input as InputPrimitive } from "@base-ui/react/input"

import { cn } from "@/lib/utils"

function Input({ className, type, ...props }: React.ComponentProps<"input">) {
    return (
        <InputPrimitive
            type={type}
            data-slot="input"
            className={cn(
                "h-control w-full min-w-0 rounded-md border border-input bg-surface-control px-2.5 py-1 text-base text-foreground transition-[background-color,border-color,box-shadow] duration-150 outline-none file:inline-flex file:h-6 file:border-0 file:bg-transparent file:text-body-compact file:font-medium file:text-foreground placeholder:text-muted-foreground/75 hover:border-primary/55 focus-visible:border-primary focus-visible:ring-[3px] focus-visible:ring-ring disabled:pointer-events-none disabled:cursor-not-allowed disabled:bg-muted disabled:text-muted-foreground disabled:opacity-100 aria-invalid:border-destructive aria-invalid:ring-[3px] aria-invalid:ring-destructive/15 md:text-body-compact dark:aria-invalid:border-destructive/70 dark:aria-invalid:ring-destructive/25",
                className,
            )}
            {...props}
        />
    )
}

export { Input }
