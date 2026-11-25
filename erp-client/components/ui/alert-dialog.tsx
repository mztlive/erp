"use client"

import * as React from "react"
import { AlertDialog as AlertDialogPrimitive } from "@base-ui/react/alert-dialog"

import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"

function AlertDialog({ ...props }: AlertDialogPrimitive.Root.Props) {
    return <AlertDialogPrimitive.Root data-slot="alert-dialog" {...props} />
}

function AlertDialogTrigger({
    id,
    ...props
}: AlertDialogPrimitive.Trigger.Props & { id?: string }) {
    return (
        <AlertDialogPrimitive.Trigger
            id={id}
            data-slot="alert-dialog-trigger"
            {...props}
        />
    )
}

function AlertDialogPortal({ ...props }: AlertDialogPrimitive.Portal.Props) {
    return (
        <AlertDialogPrimitive.Portal
            data-slot="alert-dialog-portal"
            {...props}
        />
    )
}

function AlertDialogOverlay({
    className,
    ...props
}: AlertDialogPrimitive.Backdrop.Props) {
    return (
        <AlertDialogPrimitive.Backdrop
            data-slot="alert-dialog-overlay"
            className={cn(
                "fixed inset-0 isolate z-50 bg-overlay duration-100 data-open:animate-in data-open:fade-in-0 data-closed:animate-out data-closed:fade-out-0",
                className,
            )}
            {...props}
        />
    )
}

type AlertDialogSize = "sm" | "default" | "lg" | "xl"

const ALERT_DIALOG_SIZE_CLASS: Record<AlertDialogSize, string> = {
    sm: "sm:max-w-xs",
    default: "sm:max-w-md",
    lg: "sm:max-w-xl",
    xl: "sm:max-w-4xl",
}

function AlertDialogContent({
    className,
    size = "default",
    ...props
}: AlertDialogPrimitive.Popup.Props & {
    size?: AlertDialogSize
}) {
    return (
        <AlertDialogPortal>
            <AlertDialogOverlay />
            <AlertDialogPrimitive.Popup
                data-slot="alert-dialog-content"
                data-size={size}
                data-align={size === "sm" ? "center" : "start"}
                className={cn(
                    "group/alert-dialog-content fixed top-1/2 left-1/2 z-50 grid w-full max-w-[calc(100%-2rem)] max-h-[calc(100dvh-2rem)] -translate-x-1/2 -translate-y-1/2 gap-overlay-gap overflow-y-auto overscroll-contain rounded-xl border border-border/80 bg-popover p-overlay-padding text-body-compact text-popover-foreground shadow-lg duration-100 outline-none data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95",
                    ALERT_DIALOG_SIZE_CLASS[size],
                    className,
                )}
                {...props}
            />
        </AlertDialogPortal>
    )
}

function AlertDialogHeader({
    className,
    ...props
}: React.ComponentProps<"div">) {
    return (
        <div
            data-slot="alert-dialog-header"
            className={cn(
                "grid grid-rows-[auto_1fr] place-items-center gap-1.5 text-center has-data-[slot=alert-dialog-media]:grid-rows-[auto_auto_1fr] has-data-[slot=alert-dialog-media]:gap-x-3.5 sm:group-data-[align=start]/alert-dialog-content:place-items-start sm:group-data-[align=start]/alert-dialog-content:text-left sm:group-data-[align=start]/alert-dialog-content:has-data-[slot=alert-dialog-media]:grid-rows-[auto_1fr]",
                className,
            )}
            {...props}
        />
    )
}

function AlertDialogFooter({
    className,
    ...props
}: React.ComponentProps<"div">) {
    return (
        <div
            data-slot="alert-dialog-footer"
            className={cn(
                "flex flex-col-reverse gap-2 group-data-[size=sm]/alert-dialog-content:grid group-data-[size=sm]/alert-dialog-content:grid-cols-2 sm:flex-row sm:justify-end",
                className,
            )}
            {...props}
        />
    )
}

function AlertDialogMedia({
    className,
    ...props
}: React.ComponentProps<"div">) {
    return (
        <div
            data-slot="alert-dialog-media"
            className={cn(
                "mb-1 inline-flex size-10 items-center justify-center rounded-full bg-muted sm:group-data-[align=start]/alert-dialog-content:row-span-2 *:[svg:not([class*='size-'])]:size-5",
                className,
            )}
            {...props}
        />
    )
}

function AlertDialogTitle({
    className,
    ...props
}: React.ComponentProps<typeof AlertDialogPrimitive.Title>) {
    return (
        <AlertDialogPrimitive.Title
            data-slot="alert-dialog-title"
            className={cn(
                "font-heading text-section-title font-semibold tracking-tight sm:group-data-[align=start]/alert-dialog-content:group-has-data-[slot=alert-dialog-media]/alert-dialog-content:col-start-2",
                className,
            )}
            {...props}
        />
    )
}

function AlertDialogDescription({
    className,
    ...props
}: React.ComponentProps<typeof AlertDialogPrimitive.Description>) {
    return (
        <AlertDialogPrimitive.Description
            data-slot="alert-dialog-description"
            className={cn(
                "text-body-compact text-balance text-muted-foreground md:text-pretty *:[a]:underline *:[a]:underline-offset-3 *:[a]:hover:text-foreground sm:group-data-[align=start]/alert-dialog-content:group-has-data-[slot=alert-dialog-media]/alert-dialog-content:col-start-2",
                className,
            )}
            {...props}
        />
    )
}

function AlertDialogAction({
    className,
    id,
    ...props
}: React.ComponentProps<typeof LoadingButton> & { id?: string }) {
    return (
        <LoadingButton
            id={id}
            data-slot="alert-dialog-action"
            className={cn(className)}
            {...props}
        />
    )
}

function AlertDialogCancel({
    className,
    variant = "outline",
    size = "default",
    id,
    ...props
}: AlertDialogPrimitive.Close.Props &
    Pick<React.ComponentProps<typeof Button>, "variant" | "size"> & {
        id?: string
    }) {
    return (
        <AlertDialogPrimitive.Close
            id={id}
            data-slot="alert-dialog-cancel"
            className={cn(className)}
            render={<Button variant={variant} size={size} />}
            {...props}
        />
    )
}

export {
    AlertDialog,
    AlertDialogAction,
    AlertDialogCancel,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogMedia,
    AlertDialogOverlay,
    AlertDialogPortal,
    AlertDialogTitle,
    AlertDialogTrigger,
    type AlertDialogSize,
}
