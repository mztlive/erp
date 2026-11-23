"use client"

import * as React from "react"
import { Popover as PopoverPrimitive } from "@base-ui/react/popover"

import { useHostPopoverDismiss } from "@/hooks/use-host-popover-dismiss"
import { cn } from "@/lib/utils"

const PopoverIframeDismissContext =
    React.createContext<React.RefObject<boolean> | null>(null)

function Popover({
    open: openProp,
    defaultOpen = false,
    onOpenChange,
    actionsRef: callerActionsRef,
    ...props
}: PopoverPrimitive.Root.Props) {
    const [uncontrolledOpen, setUncontrolledOpen] =
        React.useState<boolean>(defaultOpen)
    const isControlled = openProp !== undefined
    const isOpen = isControlled ? openProp : uncontrolledOpen
    const internalActionsRef =
        React.useRef<PopoverPrimitive.Root.Actions | null>(null)
    const skipReturnFocusRef = React.useRef(false)

    React.useImperativeHandle(
        callerActionsRef,
        () => ({
            close: () => internalActionsRef.current?.close(),
            unmount: () => internalActionsRef.current?.unmount(),
        }),
        [],
    )

    const handleOpenChange = React.useCallback<
        NonNullable<PopoverPrimitive.Root.Props["onOpenChange"]>
    >(
        (nextOpen, eventDetails) => {
            if (nextOpen) {
                skipReturnFocusRef.current = false
            }
            if (!isControlled) {
                setUncontrolledOpen(nextOpen)
            }
            onOpenChange?.(nextOpen, eventDetails)
        },
        [isControlled, onOpenChange],
    )

    const handleHostIframeDismiss = React.useCallback(() => {
        skipReturnFocusRef.current = true
        internalActionsRef.current?.close()
    }, [])

    useHostPopoverDismiss(isOpen, handleHostIframeDismiss)

    return (
        <PopoverIframeDismissContext.Provider value={skipReturnFocusRef}>
            <PopoverPrimitive.Root
                data-slot="popover"
                open={openProp}
                defaultOpen={defaultOpen}
                onOpenChange={handleOpenChange}
                actionsRef={internalActionsRef}
                {...props}
            />
        </PopoverIframeDismissContext.Provider>
    )
}

function PopoverTrigger({ ...props }: PopoverPrimitive.Trigger.Props) {
    return <PopoverPrimitive.Trigger data-slot="popover-trigger" {...props} />
}

function PopoverContent({
    className,
    align = "center",
    alignOffset = 0,
    side = "bottom",
    sideOffset = 4,
    collisionAvoidance,
    collisionPadding,
    keepMounted,
    finalFocus,
    ...props
}: PopoverPrimitive.Popup.Props &
    Pick<
        PopoverPrimitive.Positioner.Props,
        | "align"
        | "alignOffset"
        | "side"
        | "sideOffset"
        | "collisionAvoidance"
        | "collisionPadding"
    > & { keepMounted?: boolean }) {
    const skipReturnFocusRef = React.useContext(PopoverIframeDismissContext)
    const resolvedFinalFocus = React.useCallback<
        Extract<
            NonNullable<PopoverPrimitive.Popup.Props["finalFocus"]>,
            (...args: never[]) => unknown
        >
    >(
        (closeType) => {
            if (skipReturnFocusRef?.current) {
                skipReturnFocusRef.current = false
                return false
            }
            if (typeof finalFocus === "function") {
                return finalFocus(closeType)
            }
            if (finalFocus && typeof finalFocus === "object") {
                return finalFocus.current
            }
            return finalFocus ?? true
        },
        [finalFocus, skipReturnFocusRef],
    )

    return (
        <PopoverPrimitive.Portal keepMounted={keepMounted}>
            <PopoverPrimitive.Positioner
                align={align}
                alignOffset={alignOffset}
                side={side}
                sideOffset={sideOffset}
                collisionAvoidance={collisionAvoidance}
                collisionPadding={collisionPadding}
                className="isolate z-50"
            >
                <PopoverPrimitive.Popup
                    data-slot="popover-content"
                    finalFocus={resolvedFinalFocus}
                    className={cn(
                        "z-50 flex w-72 origin-(--transform-origin) flex-col gap-3.5 rounded-lg border border-border/90 bg-popover p-4 text-body-compact text-popover-foreground shadow-md outline-hidden duration-100 data-[side=bottom]:slide-in-from-top-2 data-[side=inline-end]:slide-in-from-left-2 data-[side=inline-start]:slide-in-from-right-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95",
                        className,
                    )}
                    {...props}
                />
            </PopoverPrimitive.Positioner>
        </PopoverPrimitive.Portal>
    )
}

function PopoverHeader({ className, ...props }: React.ComponentProps<"div">) {
    return (
        <div
            data-slot="popover-header"
            className={cn("flex flex-col gap-1", className)}
            {...props}
        />
    )
}

function PopoverTitle({ className, ...props }: PopoverPrimitive.Title.Props) {
    return (
        <PopoverPrimitive.Title
            data-slot="popover-title"
            className={cn(
                "text-section-title font-semibold tracking-tight",
                className,
            )}
            {...props}
        />
    )
}

function PopoverDescription({
    className,
    ...props
}: PopoverPrimitive.Description.Props) {
    return (
        <PopoverPrimitive.Description
            data-slot="popover-description"
            className={cn("text-xs text-muted-foreground", className)}
            {...props}
        />
    )
}

export {
    Popover,
    PopoverContent,
    PopoverDescription,
    PopoverHeader,
    PopoverTitle,
    PopoverTrigger,
}
