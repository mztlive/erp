"use client"

import { Switch as SwitchPrimitive } from "@base-ui/react/switch"

import { cn } from "@/lib/utils"

function Switch({
    className,
    size = "default",
    nativeButton = true,
    // oxlint-disable-next-line jsx-a11y/control-has-associated-label -- Base UI binds the field label and switch content to this button.
    render = <button type="button" />,
    ...props
}: SwitchPrimitive.Root.Props & {
    size?: "sm" | "default"
}) {
    return (
        <SwitchPrimitive.Root
            nativeButton={nativeButton}
            render={render}
            data-slot="switch"
            data-size={size}
            className={cn(
                "peer group/switch relative inline-flex shrink-0 cursor-pointer items-center rounded-full border-0 p-0.5 transition-colors duration-150 outline-none after:absolute after:-inset-x-2 after:-inset-y-1.5 focus-visible:ring-3 focus-visible:ring-ring aria-invalid:ring-3 aria-invalid:ring-destructive/20 data-[size=default]:h-5.5 data-[size=default]:w-10 data-[size=sm]:h-4 data-[size=sm]:w-7 dark:aria-invalid:ring-destructive/40 data-checked:bg-primary hover:data-checked:bg-primary/90 data-unchecked:bg-muted-foreground/28 hover:data-unchecked:bg-muted-foreground/38 dark:data-unchecked:bg-input/60 data-disabled:cursor-not-allowed data-disabled:opacity-50",
                className,
            )}
            {...props}
        >
            <SwitchPrimitive.Thumb
                data-slot="switch-thumb"
                className="pointer-events-none block rounded-full bg-white shadow-xs ring-0 transition-transform duration-150 group-data-[size=default]/switch:size-4.5 group-data-[size=sm]/switch:size-3 group-data-[size=default]/switch:data-checked:translate-x-4.5 group-data-[size=sm]/switch:data-checked:translate-x-3 data-unchecked:translate-x-0"
            />
        </SwitchPrimitive.Root>
    )
}

export { Switch }
