"use client"

import * as React from "react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    AlertDialog,
    AlertDialogAction,
    AlertDialogCancel,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { getErrorMessage } from "@/lib/api/errors"

/** 后台删除确认：失败留在弹窗内提示，成功后关闭并执行回调。 */
export function DeleteConfirmationDialog({
    id,
    title,
    description,
    pending,
    onConfirm,
    onClose,
    onDeleted,
}: {
    id: string
    title: React.ReactNode
    description: React.ReactNode
    pending: boolean
    onConfirm: () => Promise<void>
    onClose: () => void
    onDeleted?: () => void
}) {
    const [error, setError] = React.useState<string | null>(null)

    return (
        <AlertDialog open onOpenChange={(open) => !open && onClose()}>
            <AlertDialogContent>
                <AlertDialogHeader>
                    <AlertDialogTitle>{title}</AlertDialogTitle>
                    <AlertDialogDescription>
                        {description}
                    </AlertDialogDescription>
                </AlertDialogHeader>
                {error ? (
                    <Alert variant="destructive" role="alert">
                        <AlertTitle>删除失败</AlertTitle>
                        <AlertDescription>{error}</AlertDescription>
                    </Alert>
                ) : null}
                <AlertDialogFooter>
                    <AlertDialogCancel id={`${id}-cancel`} disabled={pending}>
                        取消
                    </AlertDialogCancel>
                    <AlertDialogAction
                        loading={pending}
                        id={`${id}-confirm`}
                        variant="destructive"
                        disabled={pending}
                        onClick={async () => {
                            setError(null)
                            try {
                                await onConfirm()
                                onClose()
                                onDeleted?.()
                            } catch (cause) {
                                setError(
                                    getErrorMessage(
                                        cause,
                                        "删除失败，请重试。",
                                    ),
                                )
                            }
                        }}
                    >
                        {pending ? "删除中…" : "确认删除"}
                    </AlertDialogAction>
                </AlertDialogFooter>
            </AlertDialogContent>
        </AlertDialog>
    )
}
