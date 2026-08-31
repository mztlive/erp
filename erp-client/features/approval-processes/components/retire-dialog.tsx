"use client"

import * as React from "react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"

import {
    definitionErrorMessage,
    isDefinitionVersionConflict,
    newCommandKey,
} from "../errors"
import { documentTypeLabel, versionLabel } from "../labels"
import { useRetireDefinitionMutation } from "../queries"
import type { DefinitionDetailView } from "../types"
import { buildLockRequest } from "../write-payload"

/**
 * 退役确认。退役后若无发布版本，该类型新单据创建将失败关闭。
 */
export function RetireDialog({
    detail,
    lockVersion,
    open,
    onOpenChange,
    onConflict,
    onRetired,
    id = "governance-approval-processes-retire-dialog",
}: {
    detail: DefinitionDetailView
    lockVersion: string
    open: boolean
    onOpenChange: (open: boolean) => void
    onConflict: () => void
    onRetired: () => void
    id?: string
}) {
    const retire = useRetireDefinitionMutation()
    const [error, setError] = React.useState<string | null>(null)
    const [commandKey, setCommandKey] = React.useState(() =>
        newCommandKey("retire"),
    )

    React.useEffect(() => {
        if (!open) return
        setError(null)
        setCommandKey(newCommandKey("retire"))
    }, [open, lockVersion])

    const handleRetire = async () => {
        setError(null)
        try {
            await retire.mutateAsync({
                definitionId: detail.definition_id,
                request: buildLockRequest(lockVersion, commandKey),
            })
            onOpenChange(false)
            onRetired()
        } catch (cause) {
            setError(definitionErrorMessage(cause))
            if (isDefinitionVersionConflict(cause)) {
                setCommandKey(newCommandKey("retire"))
                onConflict()
            }
        }
    }

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent closeButtonId={`${id}-close`}>
                <DialogHeader>
                    <DialogTitle>退役审批流程</DialogTitle>
                    <DialogDescription>
                        {documentTypeLabel(
                            detail.document_type,
                            detail.document_type_label,
                        )}
                        · {versionLabel(detail.definition_version)}
                    </DialogDescription>
                </DialogHeader>
                <p className="text-sm">
                    退役只影响新单据绑定。已有单据和进行中的审批仍使用原版本。
                    退役后若没有新的已发布版本，该单据类型将无法创建新单据。
                </p>
                {error ? (
                    <Alert variant="destructive">
                        <AlertTitle>未能退役</AlertTitle>
                        <AlertDescription>{error}</AlertDescription>
                    </Alert>
                ) : null}
                <DialogFooter>
                    <Button
                        id={`${id}-cancel`}
                        type="button"
                        variant="outline"
                        disabled={retire.isPending}
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                    <Button
                        id={`${id}-confirm`}
                        type="button"
                        variant="destructive"
                        disabled={retire.isPending}
                        onClick={() => void handleRetire()}
                    >
                        {retire.isPending ? "正在退役…" : "确认退役"}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
