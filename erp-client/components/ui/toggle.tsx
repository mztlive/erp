"use client"

import { Toggle as TogglePrimitive } from "@base-ui/react/toggle"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"

const toggleVariants = cva(
    "group/toggle inline-flex items-center justify-center gap-1 rounded-md text-body-compact font-medium whitespace-nowrap transition-colors outline-none hover:bg-accent hover:text-foreground focus-visible:border-primary focus-visible:ring-[3px] focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-destructive/20 aria-pressed:bg-sidebar-accent aria-pressed:text-sidebar-accent-foreground dark:aria-invalid:ring-destructive/40 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
    {
        variants: {
            variant: {
                default: "bg-transparent",
                outline:
                    "border border-input bg-card hover:border-primary/45 hover:bg-accent/50",
            },
            size: {
                default:
                    "h-control min-w-control px-2.5 has-data-[icon=inline-end]:pr-2 has-data-[icon=inline-start]:pl-2",
                sm: "h-control-sm min-w-control-sm px-2 has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5",
                lg: "h-control-lg min-w-control-lg px-3 has-data-[icon=inline-end]:pr-2 has-data-[icon=inline-start]:pl-2",
            },
        },
        defaultVariants: {
            variant: "default",
            size: "default",
        },
    },
)

function Toggle({
    className,
    variant = "default",
    size = "default",
    id,
    ...props
}: TogglePrimitive.Props &
    VariantProps<typeof toggleVariants> & { id?: string }) {
    return (
        <TogglePrimitive
            id={id}
            data-slot="toggle"
            className={cn(toggleVariants({ variant, size, className }))}
            {...props}
        />
    )
}

export { Toggle, toggleVariants }
