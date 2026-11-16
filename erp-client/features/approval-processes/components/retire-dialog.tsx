"use client"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"

import { documentTypeLabel, versionLabel } from "../labels"
import { useRetireDefinitionMutation } from "../queries"
import type { DefinitionDetailView } from "../types"
import { buildLockRequest } from "../write-payload"
import { useDefinitionLockCommand } from "./use-definition-lock-command"

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
    const { error, submit } = useDefinitionLockCommand({
        prefix: "retire",
        open,
        lockVersion,
        run: (commandKey) =>
            retire.mutateAsync({
                definitionId: detail.definition_id,
                request: buildLockRequest(lockVersion, commandKey),
            }),
        onOpenChange,
        onDone: onRetired,
        onConflict,
    })

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent closeButtonId={`${id}-close`}>
                <DialogHeader>
                    <DialogTitle>停用审批流程</DialogTitle>
                    <DialogDescription>
                        {documentTypeLabel(
                            detail.document_type,
                            detail.document_type_label,
                        )}
                        · {versionLabel(detail.definition_version)}
                    </DialogDescription>
                </DialogHeader>
                <p className="text-sm">
                    停用后，新发起的单据将不再使用此流程；已创建或审批中的单据不受影响。注意：若停用后该单据没有其他启用中的审批流程，将暂时无法新建单据。
                </p>
                {error ? (
                    <Alert variant="destructive">
                        <AlertTitle>未能停用</AlertTitle>
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
                    <LoadingButton
                        loading={retire.isPending}
                        id={`${id}-confirm`}
                        type="button"
                        variant="destructive"
                        disabled={retire.isPending}
                        onClick={() => void submit()}
                    >
                        {retire.isPending ? "正在停用…" : "确认停用"}
                    </LoadingButton>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
