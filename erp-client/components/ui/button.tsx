import { Button as ButtonPrimitive } from "@base-ui/react/button"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"

const buttonVariants = cva(
    "group/button inline-flex shrink-0 items-center justify-center rounded-lg border border-transparent bg-clip-padding text-sm font-medium whitespace-nowrap transition-all duration-150 outline-none select-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/25 active:not-aria-[haspopup]:translate-y-px active:not-aria-[haspopup]:shadow-none disabled:pointer-events-none disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
    {
        variants: {
            variant: {
                default:
                    "bg-primary font-medium text-primary-foreground shadow-xs hover:bg-primary/90 hover:shadow-sm active:bg-primary/95",
                outline:
                    "border-border/80 bg-card text-foreground shadow-xs hover:bg-accent/60 hover:text-foreground hover:border-border aria-expanded:bg-accent aria-expanded:text-foreground dark:bg-transparent dark:hover:bg-input/30",
                secondary:
                    "bg-secondary text-secondary-foreground shadow-xs hover:bg-secondary/80 aria-expanded:bg-secondary aria-expanded:text-secondary-foreground",
                ghost: "hover:bg-accent/60 hover:text-foreground aria-expanded:bg-accent aria-expanded:text-foreground dark:hover:bg-muted/50",
                destructive:
                    "bg-destructive/10 text-destructive border-destructive/20 shadow-xs hover:bg-destructive/18 focus-visible:border-destructive/40 focus-visible:ring-destructive/20 dark:bg-destructive/20 dark:hover:bg-destructive/30 dark:focus-visible:ring-destructive/40",
                link: "text-primary underline-offset-4 hover:underline",
            },
            size: {
                default:
                    "h-control gap-1.5 px-3 has-data-[icon=inline-end]:pr-2.5 has-data-[icon=inline-start]:pl-2.5",
                xs: "h-control-xs gap-1 px-2.5 text-xs has-data-[icon=inline-end]:pr-2 has-data-[icon=inline-start]:pl-2 [&_svg:not([class*='size-'])]:size-3",
                sm: "h-control-sm gap-1 px-3 has-data-[icon=inline-end]:pr-2 has-data-[icon=inline-start]:pl-2",
                lg: "h-control-lg gap-1.5 px-4 has-data-[icon=inline-end]:pr-3 has-data-[icon=inline-start]:pl-3",
                icon: "size-control",
                "icon-xs":
                    "size-control-xs [&_svg:not([class*='size-'])]:size-3",
                "icon-sm": "size-control-sm",
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
            className={cn(buttonVariants({ variant, size, className }))}
            render={render}
            nativeButton={nativeButton ?? !render}
            {...props}
        />
    )
}

export { Button, buttonVariants }
