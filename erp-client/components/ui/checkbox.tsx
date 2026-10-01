"use client"

import * as React from "react"
import { Checkbox as CheckboxPrimitive } from "@base-ui/react/checkbox"

import { cn } from "@/lib/utils"
import { CheckIcon, MinusIcon } from "lucide-react"

const checkboxControlClassName =
    "group/checkbox peer relative flex size-4 shrink-0 items-center justify-center rounded-lg border border-transparent bg-input/90 transition-shadow outline-none group-has-disabled/field:opacity-50 after:absolute after:-inset-x-3 after:-inset-y-2 focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/30 disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40"

function Checkbox({ className, ...props }: CheckboxPrimitive.Root.Props) {
    return (
        <CheckboxPrimitive.Root
            data-slot="checkbox"
            className={cn(
                checkboxControlClassName,
                "aria-invalid:aria-checked:border-primary data-checked:border-primary data-checked:bg-primary data-checked:text-primary-foreground dark:data-checked:bg-primary",
                className,
            )}
            {...props}
        >
            <CheckboxPrimitive.Indicator
                data-slot="checkbox-indicator"
                className="grid place-content-center text-current transition-none [&>svg]:size-3.5"
            >
                <CheckIcon className="group-data-indeterminate/checkbox:hidden" />
                <MinusIcon className="hidden group-data-indeterminate/checkbox:block" />
            </CheckboxPrimitive.Indicator>
        </CheckboxPrimitive.Root>
    )
}

type NativeCheckboxProps = Omit<React.ComponentProps<"input">, "type"> & {
    onCheckedChange?: (checked: boolean) => void
    /** 原生半选状态；不会改变 input.checked 的布尔值。 */
    indeterminate?: boolean
}

/** 保留原生表单与自动化 DOM 合同的复选框，外观与 Checkbox 共用主题。 */
function NativeCheckbox({
    className,
    onCheckedChange,
    onChange,
    indeterminate = false,
    ref,
    ...props
}: NativeCheckboxProps) {
    const inputRef = React.useCallback(
        (input: HTMLInputElement | null) => {
            if (input) input.indeterminate = indeterminate
            if (typeof ref === "function") return ref(input)
            if (ref) ref.current = input
        },
        [indeterminate, ref],
    )

    return (
        <span className="relative inline-flex shrink-0 align-middle">
            <input
                {...props}
                ref={inputRef}
                type="checkbox"
                data-slot="checkbox"
                className={cn(
                    checkboxControlClassName,
                    "appearance-none checked:border-primary checked:bg-primary checked:text-primary-foreground indeterminate:border-primary indeterminate:bg-primary indeterminate:text-primary-foreground aria-invalid:checked:border-primary dark:checked:bg-primary dark:indeterminate:bg-primary",
                    className,
                )}
                onChange={(event) => {
                    onChange?.(event)
                    onCheckedChange?.(event.currentTarget.checked)
                }}
            />
            <CheckIcon
                aria-hidden="true"
                className="pointer-events-none absolute inset-0 m-auto hidden size-3.5 text-primary-foreground peer-checked:block peer-indeterminate:hidden peer-disabled:opacity-50 group-has-disabled/field:opacity-50"
            />
            <MinusIcon
                aria-hidden="true"
                className="pointer-events-none absolute inset-0 m-auto hidden size-3.5 text-primary-foreground peer-indeterminate:block peer-disabled:opacity-50 group-has-disabled/field:opacity-50"
            />
        </span>
    )
}

export { Checkbox, NativeCheckbox, type NativeCheckboxProps }
