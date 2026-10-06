"use client"

import { Radio as RadioPrimitive } from "@base-ui/react/radio"
import { RadioGroup as RadioGroupPrimitive } from "@base-ui/react/radio-group"
import { CheckIcon } from "lucide-react"

import { buttonVariants } from "@/components/ui/button"
import { cn } from "@/lib/utils"

function RadioGroup({ className, ...props }: RadioGroupPrimitive.Props) {
    return (
        <RadioGroupPrimitive
            data-slot="radio-group"
            className={cn("grid w-full gap-3", className)}
            {...props}
        />
    )
}

function RadioGroupItem({
    className,
    variant = "default",
    children,
    ...props
}: RadioPrimitive.Root.Props & { variant?: "default" | "segment" }) {
    if (variant === "segment") {
        return (
            <RadioPrimitive.Root
                data-slot="radio-group-item"
                className={cn(
                    buttonVariants({ variant: "outline" }),
                    "relative rounded-none px-8 first-of-type:rounded-l-lg last-of-type:rounded-r-lg not-first-of-type:border-l-0 data-checked:bg-muted focus-visible:z-10",
                    className,
                )}
                {...props}
            >
                <RadioPrimitive.Indicator className="absolute left-3 flex items-center">
                    <CheckIcon className="size-4" aria-hidden="true" />
                </RadioPrimitive.Indicator>
                {children}
            </RadioPrimitive.Root>
        )
    }
    return (
        <RadioPrimitive.Root
            data-slot="radio-group-item"
            className={cn(
                "group/radio-group-item peer relative flex aspect-square size-4 shrink-0 rounded-2xl border border-transparent bg-input/90 outline-none after:absolute after:-inset-x-3 after:-inset-y-2 focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/30 disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40 data-checked:bg-primary data-checked:text-primary-foreground dark:data-checked:bg-primary",
                className,
            )}
            {...props}
        >
            <RadioPrimitive.Indicator
                data-slot="radio-group-indicator"
                className="flex size-4 items-center justify-center"
            >
                <span className="absolute top-1/2 left-1/2 size-2 -translate-x-1/2 -translate-y-1/2 rounded-full bg-primary-foreground dark:size-2.5" />
            </RadioPrimitive.Indicator>
        </RadioPrimitive.Root>
    )
}

export { RadioGroup, RadioGroupItem }
