"use client"
import * as React from "react"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from "@/components/ui/dialog"
import { getErrorMessage } from "@/lib/api/errors"

export type ChangeConfirmation = {
    title: string
    description: string
    details?: readonly string[]
    confirmLabel: string
    destructive?: boolean
    onConfirm: () => void | Promise<void>
}
/** 本地变更和上架操作的结构化确认，失败保留当前核对内容。 */
export function useChangeConfirmation(id: string) {
    const [request, setRequest] = React.useState<ChangeConfirmation | null>(
        null,
    )
    const [pending, setPending] = React.useState(false)
    const [error, setError] = React.useState<string | null>(null)
    const running = React.useRef(false)
    const confirm = (next: ChangeConfirmation) => {
        setError(null)
        setRequest(next)
    }
    const dialog = (
        <Dialog
            open={Boolean(request)}
            onOpenChange={(open) => {
                if (!open && !running.current) setRequest(null)
            }}
        >
            <DialogContent
                closeButtonId={`${id}-close`}
                showCloseButton={!pending}
                className="max-h-[85dvh] w-[calc(100vw-2rem)] overflow-y-auto sm:max-w-lg"
            >
                <DialogHeader>
                    <DialogTitle>{request?.title}</DialogTitle>
                    <DialogDescription>
                        {request?.description}
                    </DialogDescription>
                </DialogHeader>
                {request?.details?.length ? (
                    <ul className="max-h-60 list-disc overflow-y-auto pl-5 text-sm">
                        {request.details.map((detail, i) => (
                            <li key={i}>{detail}</li>
                        ))}
                    </ul>
                ) : null}
                {error ? (
                    <p role="alert" className="text-sm text-destructive">
                        {error}
                    </p>
                ) : null}
                <DialogFooter>
                    <Button
                        id={`${id}-cancel`}
                        variant="outline"
                        disabled={pending}
                        onClick={() => setRequest(null)}
                    >
                        取消
                    </Button>
                    <Button
                        id={`${id}-confirm`}
                        variant={
                            request?.destructive ? "destructive" : "default"
                        }
                        disabled={pending}
                        onClick={async () => {
                            if (!request || running.current) return
                            running.current = true
                            setPending(true)
                            setError(null)
                            try {
                                await request.onConfirm()
                                setRequest(null)
                            } catch (cause) {
                                setError(
                                    getErrorMessage(cause, "操作失败，请重试"),
                                )
                            } finally {
                                running.current = false
                                setPending(false)
                            }
                        }}
                    >
                        {pending ? "处理中…" : request?.confirmLabel}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
    return { confirm, dialog }
}
