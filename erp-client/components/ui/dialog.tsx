"use client"

import * as React from "react"
import { Dialog as DialogPrimitive } from "@base-ui/react/dialog"

import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { XIcon } from "lucide-react"

function Dialog({ ...props }: DialogPrimitive.Root.Props) {
    return <DialogPrimitive.Root data-slot="dialog" {...props} />
}

function DialogTrigger({
    id,
    ...props
}: DialogPrimitive.Trigger.Props & { id?: string }) {
    return (
        <DialogPrimitive.Trigger
            id={id}
            data-slot="dialog-trigger"
            {...props}
        />
    )
}

function DialogPortal({ ...props }: DialogPrimitive.Portal.Props) {
    return <DialogPrimitive.Portal data-slot="dialog-portal" {...props} />
}

function DialogClose({ ...props }: DialogPrimitive.Close.Props) {
    return <DialogPrimitive.Close data-slot="dialog-close" {...props} />
}

function DialogOverlay({
    className,
    ...props
}: DialogPrimitive.Backdrop.Props) {
    return (
        <DialogPrimitive.Backdrop
            data-slot="dialog-overlay"
            className={cn(
                "fixed inset-0 isolate z-50 bg-overlay transition-opacity duration-200 ease-out data-ending-style:opacity-0 data-starting-style:opacity-0",
                className,
            )}
            {...props}
        />
    )
}

type DialogSize = "sm" | "md" | "lg" | "xl" | "2xl" | "full"
type DialogScroll = "body" | "popup"

type DialogContentProps = DialogPrimitive.Popup.Props & {
    size?: DialogSize
    /**
     * `"popup"`（默认）：弹窗整体滚动，兼容未包 `DialogBody` 的存量弹窗。
     * `"body"`：固定 `DialogHeader`/`DialogFooter` 槽位，由 `DialogBody` 独立滚动。
     */
    scroll?: DialogScroll
    showCloseButton?: boolean
    closeButtonId?: string
    forceOverlay?: boolean
}

const DIALOG_SIZE_CLASS: Record<DialogSize, string> = {
    sm: "sm:max-w-md max-h-[calc(100dvh-2rem)]",
    md: "sm:max-w-lg max-h-[calc(100dvh-2rem)]",
    lg: "sm:max-w-2xl max-h-[85dvh]",
    xl: "sm:max-w-4xl max-h-[88dvh]",
    "2xl": "sm:max-w-6xl max-h-[92dvh]",
    full: "inset-0 h-dvh max-h-dvh w-screen max-w-none rounded-none border-0 shadow-none",
}

type DialogLayoutContextValue = {
    showCloseButton: boolean
    scroll: DialogScroll
}

const DialogLayoutContext = React.createContext<DialogLayoutContextValue>({
    showCloseButton: false,
    scroll: "popup",
})

function DialogContent({
    className,
    children,
    size = "md",
    scroll = "popup",
    showCloseButton = true,
    closeButtonId,
    forceOverlay = false,
    ...props
}: DialogContentProps) {
    const layoutContext = React.useMemo(
        () => ({ showCloseButton, scroll }),
        [showCloseButton, scroll],
    )

    return (
        <DialogPortal>
            <DialogOverlay forceRender={forceOverlay} />
            <DialogPrimitive.Popup
                data-slot="dialog-content"
                data-size={size}
                data-scroll={scroll}
                className={cn(
                    "fixed z-50 overscroll-contain bg-popover text-body-compact text-popover-foreground outline-none",
                    size === "full"
                        ? "transition-[translate,transform,opacity] duration-250 ease-[cubic-bezier(0.22,1,0.36,1)] data-ending-style:translate-x-full data-ending-style:opacity-0 data-starting-style:translate-x-full data-starting-style:opacity-0"
                        : "top-1/2 left-1/2 w-full max-w-[calc(100%-2rem)] -translate-x-1/2 -translate-y-1/2 rounded-xl border border-border/80 shadow-lg duration-100 data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95",
                    DIALOG_SIZE_CLASS[size],
                    scroll === "body"
                        ? "flex flex-col overflow-hidden p-0"
                        : "grid gap-overlay-gap overflow-y-auto p-overlay-padding",
                    className,
                )}
                {...props}
            >
                <DialogLayoutContext.Provider value={layoutContext}>
                    {children}
                </DialogLayoutContext.Provider>
                {showCloseButton && (
                    <DialogPrimitive.Close
                        id={closeButtonId}
                        data-slot="dialog-close"
                        render={
                            <Button
                                variant="ghost"
                                className="absolute top-3.5 right-3.5 text-muted-foreground hover:text-foreground"
                                size="icon-sm"
                            />
                        }
                    >
                        <XIcon />
                        <span className="sr-only">关闭</span>
                    </DialogPrimitive.Close>
                )}
            </DialogPrimitive.Popup>
        </DialogPortal>
    )
}

function DialogHeader({ className, ...props }: React.ComponentProps<"div">) {
    const { showCloseButton, scroll } = React.useContext(DialogLayoutContext)
    return (
        <div
            data-slot="dialog-header"
            className={cn(
                "flex flex-col gap-1.5",
                scroll === "body"
                    ? cn(
                          "shrink-0 border-b border-border/80 py-3.5",
                          showCloseButton
                              ? "pl-overlay-padding pr-12"
                              : "px-overlay-padding",
                      )
                    : showCloseButton && "pr-8",
                className,
            )}
            {...props}
        />
    )
}

function DialogBody({ className, ...props }: React.ComponentProps<"div">) {
    const { scroll } = React.useContext(DialogLayoutContext)
    return (
        <div
            data-slot="dialog-body"
            className={cn(
                "min-h-0 flex-1 overflow-y-auto overscroll-contain",
                scroll === "body" && "px-overlay-padding py-4",
                className,
            )}
            {...props}
        />
    )
}

function DialogFooter({
    className,
    showCloseButton = false,
    closeButtonId,
    children,
    ...props
}: React.ComponentProps<"div"> & {
    showCloseButton?: boolean
    closeButtonId?: string
}) {
    const { scroll } = React.useContext(DialogLayoutContext)
    return (
        <div
            data-slot="dialog-footer"
            className={cn(
                "flex flex-col-reverse gap-2 sm:flex-row sm:justify-end",
                scroll === "body" &&
                    "shrink-0 border-t border-border/80 px-overlay-padding py-3",
                className,
            )}
            {...props}
        >
            {children}
            {showCloseButton && (
                <DialogPrimitive.Close
                    id={closeButtonId}
                    render={<Button variant="outline" />}
                >
                    关闭
                </DialogPrimitive.Close>
            )}
        </div>
    )
}

function DialogTitle({ className, ...props }: DialogPrimitive.Title.Props) {
    return (
        <DialogPrimitive.Title
            data-slot="dialog-title"
            className={cn(
                "font-heading text-section-title font-semibold tracking-tight",
                className,
            )}
            {...props}
        />
    )
}

function DialogDescription({
    className,
    ...props
}: DialogPrimitive.Description.Props) {
    return (
        <DialogPrimitive.Description
            data-slot="dialog-description"
            className={cn(
                "text-xs text-muted-foreground *:[a]:underline *:[a]:underline-offset-3 *:[a]:hover:text-foreground",
                className,
            )}
            {...props}
        />
    )
}

export {
    Dialog,
    DialogBody,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogOverlay,
    DialogPortal,
    DialogTitle,
    DialogTrigger,
    type DialogContentProps,
    type DialogScroll,
    type DialogSize,
}
