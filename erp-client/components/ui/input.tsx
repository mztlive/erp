import * as React from "react"
import { Input as InputPrimitive } from "@base-ui/react/input"

import { cn } from "@/lib/utils"

const inputSizeClasses = {
    sm: "h-control-sm",
    default: "h-control",
    lg: "h-control-lg",
} as const

type InputSize = keyof typeof inputSizeClasses

type InputProps = Omit<React.ComponentProps<"input">, "size"> & {
    /**
     * 控件高度档位（`sm` 28px / `default` 32px / `lg` 36px）。
     * 兼容透传原生 `<input size={number}>`（如 `InputGroupInput`），数值将透传给原生 `size` 属性。
     */
    size?: InputSize | number
    /** 原生 `<input size>` 字符宽度属性。 */
    inputSize?: number
}

function Input({
    className,
    type,
    size = "default",
    inputSize,
    ...props
}: InputProps) {
    const resolvedSize: InputSize = typeof size === "string" ? size : "default"
    const htmlSize = inputSize ?? (typeof size === "number" ? size : undefined)

    return (
        <InputPrimitive
            type={type}
            size={htmlSize}
            data-slot="input"
            data-size={resolvedSize}
            className={cn(
                "w-full min-w-0 rounded-md border border-input bg-surface-control px-2.5 py-1 text-base text-foreground transition-[background-color,border-color,box-shadow] duration-150 outline-none file:inline-flex file:h-6 file:border-0 file:bg-transparent file:text-body-compact file:font-medium file:text-foreground placeholder:text-muted-foreground/75 hover:border-primary/55 focus-visible:border-primary focus-visible:ring-3 focus-visible:ring-ring disabled:pointer-events-none disabled:cursor-not-allowed disabled:bg-muted disabled:text-muted-foreground disabled:opacity-100 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/15 md:text-body-compact dark:aria-invalid:border-destructive/70 dark:aria-invalid:ring-destructive/25",
                inputSizeClasses[resolvedSize],
                className,
            )}
            {...props}
        />
    )
}

export { Input, type InputProps, type InputSize }
