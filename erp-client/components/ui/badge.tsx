import { mergeProps } from "@base-ui/react/merge-props"
import { useRender } from "@base-ui/react/use-render"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"

const badgeVariants = cva(
    "group/badge inline-flex h-5.5 w-fit shrink-0 items-center justify-center gap-1 overflow-hidden rounded-sm border border-transparent px-2 py-0 text-tiny font-medium whitespace-nowrap transition-colors focus-visible:border-primary focus-visible:ring-3 focus-visible:ring-ring has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 aria-invalid:border-destructive aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 [&>svg]:pointer-events-none [&>svg]:size-3!",
    {
        variants: {
            variant: {
                default:
                    "bg-primary text-primary-foreground [a]:hover:bg-primary/80",
                secondary:
                    "bg-secondary text-secondary-foreground [a]:hover:bg-secondary/80",
                destructive:
                    "bg-destructive-soft text-destructive-soft-foreground focus-visible:ring-destructive/20 dark:focus-visible:ring-destructive/40 [a]:hover:bg-destructive-soft/70",
                success:
                    "bg-success-soft text-success-soft-foreground [a]:hover:bg-success-soft/70",
                warning:
                    "bg-warning-soft text-warning-soft-foreground [a]:hover:bg-warning-soft/70",
                info: "bg-info-soft text-info-soft-foreground [a]:hover:bg-info-soft/70",
                orange: "bg-orange-soft text-orange-soft-foreground [a]:hover:bg-orange-soft/70",
                teal: "bg-teal-soft text-teal-soft-foreground [a]:hover:bg-teal-soft/70",
                violet: "bg-violet-soft text-violet-soft-foreground [a]:hover:bg-violet-soft/70",
                lime: "bg-lime-soft text-lime-soft-foreground [a]:hover:bg-lime-soft/70",
                rose: "bg-rose-soft text-rose-soft-foreground [a]:hover:bg-rose-soft/70",
                indigo: "bg-indigo-soft text-indigo-soft-foreground [a]:hover:bg-indigo-soft/70",
                cyan: "bg-cyan-soft text-cyan-soft-foreground [a]:hover:bg-cyan-soft/70",
                neutral:
                    "bg-neutral-soft text-neutral-soft-foreground [a]:hover:bg-neutral-soft/70",
                outline:
                    "border-border text-foreground [a]:hover:bg-muted [a]:hover:text-muted-foreground",
                ghost: "hover:bg-muted hover:text-muted-foreground dark:hover:bg-muted/50",
                link: "text-primary underline-offset-4 hover:underline",
            },
        },
        defaultVariants: {
            variant: "default",
        },
    },
)

function Badge({
    className,
    variant = "default",
    render,
    ...props
}: useRender.ComponentProps<"span"> & VariantProps<typeof badgeVariants>) {
    return useRender({
        defaultTagName: "span",
        props: mergeProps<"span">(
            {
                className: cn(badgeVariants({ variant }), className),
            },
            props,
        ),
        render,
        state: {
            slot: "badge",
            variant,
        },
    })
}

export { Badge, badgeVariants }
