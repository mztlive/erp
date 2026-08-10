import * as React from "react"

import { cn } from "@/lib/utils"

function Textarea({ className, ...props }: React.ComponentProps<"textarea">) {
  return (
    <textarea
      data-slot="textarea"
      className={cn(
        "flex field-sizing-content min-h-20 w-full resize-none rounded-lg border border-input bg-surface-control px-3 py-2.5 text-base shadow-xs transition-[background-color,border-color,box-shadow] duration-200 outline-none placeholder:text-muted-foreground/80 hover:border-foreground/25 hover:bg-card focus-visible:border-ring focus-visible:bg-card focus-visible:ring-[3px] focus-visible:ring-ring/20 disabled:cursor-not-allowed disabled:bg-muted disabled:text-muted-foreground disabled:opacity-100 aria-invalid:border-destructive aria-invalid:bg-destructive-soft/30 aria-invalid:ring-[3px] aria-invalid:ring-destructive/15 md:text-sm dark:aria-invalid:border-destructive/70 dark:aria-invalid:ring-destructive/25",
        className
      )}
      {...props}
    />
  )
}

export { Textarea }
