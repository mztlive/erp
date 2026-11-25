import { Button as ButtonPrimitive } from "@base-ui/react/button"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"

const buttonVariants = cva(
    "group/button inline-flex shrink-0 items-center justify-center rounded-md border border-transparent bg-clip-padding text-body-compact font-medium whitespace-nowrap transition-colors duration-150 outline-none select-none focus-visible:border-primary focus-visible:ring-3 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
    {
        variants: {
            variant: {
                default:
                    "bg-primary font-medium text-primary-foreground shadow-none hover:bg-primary/90 active:bg-primary/85",
                outline:
                    "border-border bg-card text-foreground hover:border-primary/45 hover:bg-sidebar-accent/40 hover:text-primary aria-expanded:border-primary/50 aria-expanded:bg-sidebar-accent/60 aria-expanded:text-sidebar-accent-foreground dark:bg-transparent dark:hover:bg-input/30",
                control:
                    "border-input bg-surface-control font-normal text-foreground shadow-none hover:border-primary/55 disabled:bg-muted disabled:text-muted-foreground disabled:opacity-100 data-disabled:bg-muted data-disabled:text-muted-foreground data-disabled:opacity-100",
                secondary:
                    "bg-secondary text-secondary-foreground hover:bg-secondary/80 aria-expanded:bg-secondary aria-expanded:text-secondary-foreground",
                ghost: "hover:bg-accent/80 hover:text-foreground aria-expanded:bg-sidebar-accent/70 aria-expanded:text-sidebar-accent-foreground dark:hover:bg-muted/50",
                destructive:
                    "bg-destructive-soft text-destructive-soft-foreground border-destructive-border hover:bg-destructive/15 focus-visible:border-destructive/50 focus-visible:ring-destructive/20 dark:hover:bg-destructive/30 dark:focus-visible:ring-destructive/40",
                link: "text-primary underline-offset-4 hover:underline",
            },
            size: {
                default:
                    "h-control gap-1.5 px-3 has-data-[icon=inline-end]:pr-2.5 has-data-[icon=inline-start]:pl-2.5",
                xs: "h-control-xs gap-1 px-2 text-xs has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 [&_svg:not([class*='size-'])]:size-3",
                sm: "h-control-sm gap-1 px-2.5 text-body-compact has-data-[icon=inline-end]:pr-2 has-data-[icon=inline-start]:pl-2 [&_svg:not([class*='size-'])]:size-3.5",
                lg: "h-control-lg gap-1.5 px-4 has-data-[icon=inline-end]:pr-3 has-data-[icon=inline-start]:pl-3",
                icon: "size-control",
                "icon-xs":
                    "size-control-xs [&_svg:not([class*='size-'])]:size-3",
                "icon-sm":
                    "size-control-sm [&_svg:not([class*='size-'])]:size-3.5",
                "icon-lg": "size-control-lg",
            },
        },
        defaultVariants: {
            variant: "default",
            size: "default",
        },
    },
)

function Button({
    className,
    variant = "default",
    size = "default",
    render,
    nativeButton,
    ...props
}: ButtonPrimitive.Props & VariantProps<typeof buttonVariants>) {
    return (
        <ButtonPrimitive
            data-slot="button"
            data-variant={variant}
            className={cn(buttonVariants({ variant, size, className }))}
            render={render}
            nativeButton={nativeButton ?? !render}
            {...props}
        />
    )
}

export { Button, buttonVariants }
