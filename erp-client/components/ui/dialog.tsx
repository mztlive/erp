"use client"

import * as React from "react"
import { Dialog as DialogPrimitive } from "@base-ui/react/dialog"

import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { XIcon } from "lucide-react"

const DIALOG_BASE_Z_INDEX = 50
const DIALOG_VIEWPORT_GAP = 8
const DIALOG_DRAG_HANDLE = '[data-slot="dialog-header"]'
const DIALOG_DRAG_BLOCKER =
  'button, a, input, select, textarea, [contenteditable="true"], [data-dialog-drag-disabled]'

let topDialogZIndex = DIALOG_BASE_Z_INDEX
let activeDialogId: symbol | null = null

type DialogPosition = {
  x: number
  y: number
}

type DialogDragState = {
  element: HTMLDivElement
  frameId: number | null
  nextPosition: DialogPosition
  pointerId: number
  pointerX: number
  pointerY: number
  position: DialogPosition
  rect: DOMRect
}

function dialogTranslate(position: DialogPosition) {
  const axis = (offset: number) =>
    offset === 0
      ? "-50%"
      : `calc(-50% ${offset < 0 ? "-" : "+"} ${Math.abs(offset)}px)`

  return `${axis(position.x)} ${axis(position.y)}`
}

type DialogProps = Omit<
  DialogPrimitive.Root.Props,
  "disablePointerDismissal" | "modal"
>

function Dialog({ onOpenChange, ...props }: DialogProps) {
  const handleOpenChange: NonNullable<
    DialogPrimitive.Root.Props["onOpenChange"]
  > = (open, eventDetails) => {
    if (
      !open &&
      eventDetails.reason !== "close-press" &&
      eventDetails.reason !== "imperative-action"
    ) {
      eventDetails.cancel()
      return
    }

    onOpenChange?.(open, eventDetails)
  }

  return (
    <DialogPrimitive.Root
      data-slot="dialog"
      disablePointerDismissal
      modal={false}
      onOpenChange={handleOpenChange}
      {...props}
    />
  )
}

function DialogTrigger({ ...props }: DialogPrimitive.Trigger.Props) {
  return <DialogPrimitive.Trigger data-slot="dialog-trigger" {...props} />
}

function DialogPortal({ ...props }: DialogPrimitive.Portal.Props) {
  return <DialogPrimitive.Portal data-slot="dialog-portal" {...props} />
}

function DialogClose({ ...props }: DialogPrimitive.Close.Props) {
  return <DialogPrimitive.Close data-slot="dialog-close" {...props} />
}

function DialogContent({
  className,
  children,
  showCloseButton = true,
  style,
  onFocusCapture,
  onPointerDownCapture,
  onPointerMoveCapture,
  onPointerUpCapture,
  onPointerCancelCapture,
  onLostPointerCapture,
  ...props
}: DialogPrimitive.Popup.Props & {
  showCloseButton?: boolean
}) {
  const dialogId = React.useRef(Symbol("dialog"))
  const dragState = React.useRef<DialogDragState | null>(null)
  const [dragging, setDragging] = React.useState(false)
  const [position, setPosition] = React.useState<DialogPosition>({ x: 0, y: 0 })
  const positionRef = React.useRef(position)
  const [zIndex, setZIndex] = React.useState(DIALOG_BASE_Z_INDEX)

  const bringToFront = React.useCallback(() => {
    if (activeDialogId === dialogId.current) {
      return
    }

    activeDialogId = dialogId.current
    topDialogZIndex += 1
    setZIndex(topDialogZIndex)
  }, [])

  React.useEffect(
    () => () => {
      if (activeDialogId === dialogId.current) {
        activeDialogId = null
      }

      const frameId = dragState.current?.frameId
      if (typeof frameId === "number") {
        cancelAnimationFrame(frameId)
      }
    },
    []
  )

  const finishDragging = React.useCallback(
    (element: HTMLDivElement, pointerId: number) => {
      if (dragState.current?.pointerId !== pointerId) {
        return
      }

      const drag = dragState.current
      if (drag.frameId !== null) {
        cancelAnimationFrame(drag.frameId)
      }
      element.style.translate = dialogTranslate(drag.nextPosition)
      positionRef.current = drag.nextPosition
      setPosition(drag.nextPosition)
      dragState.current = null
      setDragging(false)
      if (element.hasPointerCapture(pointerId)) {
        element.releasePointerCapture(pointerId)
      }
    },
    []
  )

  const handleFocusCapture: NonNullable<
    DialogPrimitive.Popup.Props["onFocusCapture"]
  > = (event) => {
    onFocusCapture?.(event)
    if (!event.defaultPrevented) {
      bringToFront()
    }
  }

  const handlePointerDownCapture: NonNullable<
    DialogPrimitive.Popup.Props["onPointerDownCapture"]
  > = (event) => {
    onPointerDownCapture?.(event)
    if (event.defaultPrevented || event.button !== 0) {
      return
    }

    bringToFront()

    const target = event.target
    if (!(target instanceof Element)) {
      return
    }

    const dragHandle = target.closest(DIALOG_DRAG_HANDLE)
    if (
      !dragHandle ||
      !event.currentTarget.contains(dragHandle) ||
      target.closest(DIALOG_DRAG_BLOCKER)
    ) {
      return
    }

    const currentPosition = positionRef.current
    dragState.current = {
      element: event.currentTarget,
      frameId: null,
      nextPosition: currentPosition,
      pointerId: event.pointerId,
      pointerX: event.clientX,
      pointerY: event.clientY,
      position: currentPosition,
      rect: event.currentTarget.getBoundingClientRect(),
    }
    event.currentTarget.setPointerCapture(event.pointerId)
    setDragging(true)
    event.preventDefault()
  }

  const handlePointerMoveCapture: NonNullable<
    DialogPrimitive.Popup.Props["onPointerMoveCapture"]
  > = (event) => {
    onPointerMoveCapture?.(event)
    const drag = dragState.current
    if (!drag || drag.pointerId !== event.pointerId) {
      return
    }

    const nextLeft = drag.rect.left + event.clientX - drag.pointerX
    const nextTop = drag.rect.top + event.clientY - drag.pointerY
    const maxLeft = Math.max(
      DIALOG_VIEWPORT_GAP,
      window.innerWidth - drag.rect.width - DIALOG_VIEWPORT_GAP
    )
    const maxTop = Math.max(
      DIALOG_VIEWPORT_GAP,
      window.innerHeight - drag.rect.height - DIALOG_VIEWPORT_GAP
    )
    const left = Math.min(
      Math.max(nextLeft, DIALOG_VIEWPORT_GAP),
      maxLeft
    )
    const top = Math.min(Math.max(nextTop, DIALOG_VIEWPORT_GAP), maxTop)

    drag.nextPosition = {
      x: drag.position.x + left - drag.rect.left,
      y: drag.position.y + top - drag.rect.top,
    }
    if (drag.frameId === null) {
      drag.frameId = requestAnimationFrame(() => {
        drag.frameId = null
        if (dragState.current === drag) {
          drag.element.style.translate = dialogTranslate(drag.nextPosition)
        }
      })
    }
    event.preventDefault()
  }

  const handlePointerUpCapture: NonNullable<
    DialogPrimitive.Popup.Props["onPointerUpCapture"]
  > = (event) => {
    onPointerUpCapture?.(event)
    finishDragging(event.currentTarget, event.pointerId)
  }

  const handlePointerCancelCapture: NonNullable<
    DialogPrimitive.Popup.Props["onPointerCancelCapture"]
  > = (event) => {
    onPointerCancelCapture?.(event)
    finishDragging(event.currentTarget, event.pointerId)
  }

  const handleLostPointerCapture: NonNullable<
    DialogPrimitive.Popup.Props["onLostPointerCapture"]
  > = (event) => {
    onLostPointerCapture?.(event)
    if (dragState.current?.pointerId === event.pointerId) {
      finishDragging(event.currentTarget, event.pointerId)
    }
  }

  return (
    <DialogPortal>
      <DialogPrimitive.Popup
        data-slot="dialog-content"
        className={cn(
          "fixed top-1/2 left-1/2 z-50 grid w-full max-w-[calc(100%-2rem)] gap-6 rounded-[min(var(--radius-4xl),24px)] bg-popover p-6 text-sm text-popover-foreground shadow-xl ring-1 ring-foreground/5 duration-100 outline-none sm:max-w-md dark:ring-foreground/10 data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95",
          className,
          dragging &&
            "cursor-grabbing select-none transition-none will-change-transform"
        )}
        style={{
          ...style,
          translate: dialogTranslate(
            dragState.current?.nextPosition ?? position
          ),
          zIndex,
        }}
        onFocusCapture={handleFocusCapture}
        onPointerDownCapture={handlePointerDownCapture}
        onPointerMoveCapture={handlePointerMoveCapture}
        onPointerUpCapture={handlePointerUpCapture}
        onPointerCancelCapture={handlePointerCancelCapture}
        onLostPointerCapture={handleLostPointerCapture}
        {...props}
      >
        {children}
        {showCloseButton && (
          <DialogPrimitive.Close
            data-slot="dialog-close"
            render={
              <Button
                variant="ghost"
                className="absolute top-4 right-4 bg-secondary"
                size="icon-sm"
              />
            }
          >
            <XIcon
            />
            <span className="sr-only">关闭</span>
          </DialogPrimitive.Close>
        )}
      </DialogPrimitive.Popup>
    </DialogPortal>
  )
}

function DialogHeader({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="dialog-header"
      className={cn(
        "flex touch-none cursor-move select-none flex-col gap-1.5",
        className
      )}
      {...props}
    />
  )
}

function DialogFooter({
  className,
  showCloseButton = false,
  children,
  ...props
}: React.ComponentProps<"div"> & {
  showCloseButton?: boolean
}) {
  return (
    <div
      data-slot="dialog-footer"
      className={cn(
        "flex flex-col-reverse gap-2 sm:flex-row sm:justify-end",
        className
      )}
      {...props}
    >
      {children}
      {showCloseButton && (
        <DialogPrimitive.Close render={<Button variant="outline" />}>
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
        "font-heading text-base leading-none font-medium",
        className
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
        "text-sm text-muted-foreground *:[a]:underline *:[a]:underline-offset-3 *:[a]:hover:text-foreground",
        className
      )}
      {...props}
    />
  )
}

export {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogPortal,
  DialogTitle,
  DialogTrigger,
}
