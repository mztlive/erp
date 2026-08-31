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
import {
    documentTypeLabel,
    publishPathPreview,
    REJECT_RESTART_COPY,
    versionLabel,
} from "../labels"
import { usePublishDefinitionMutation } from "../queries"
import type { DefinitionDetailView } from "../types"
import { buildLockRequest } from "../write-payload"

/**
 * 发布确认。展示最终线性路径和固定驳回说明；409 时关闭提交态并保留输入。
 */
export function PublishDialog({
    detail,
    lockVersion,
    open,
    onOpenChange,
    onConflict,
    onPublished,
    id = "governance-approval-processes-publish-dialog",
}: {
    detail: DefinitionDetailView
    lockVersion: string
    open: boolean
    onOpenChange: (open: boolean) => void
    onConflict: () => void
    onPublished: () => void
    id?: string
}) {
    const publish = usePublishDefinitionMutation()
    const [error, setError] = React.useState<string | null>(null)
    const [commandKey, setCommandKey] = React.useState(() =>
        newCommandKey("publish"),
    )

    React.useEffect(() => {
        if (!open) return
        setError(null)
        setCommandKey(newCommandKey("publish"))
    }, [open, lockVersion])

    const path = publishPathPreview(
        detail.nodes
            .slice()
            .sort((left, right) => left.display_order - right.display_order)
            .map((node) => node.assignee_name_snapshot || node.node_name),
    )

    const handlePublish = async () => {
        setError(null)
        try {
            await publish.mutateAsync({
                definitionId: detail.definition_id,
                request: buildLockRequest(lockVersion, commandKey),
            })
            onOpenChange(false)
            onPublished()
        } catch (cause) {
            setError(definitionErrorMessage(cause))
            if (isDefinitionVersionConflict(cause)) {
                setCommandKey(newCommandKey("publish"))
                onConflict()
            }
        }
    }

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent closeButtonId={`${id}-close`}>
                <DialogHeader>
                    <DialogTitle>发布审批流程</DialogTitle>
                    <DialogDescription>
                        {documentTypeLabel(
                            detail.document_type,
                            detail.document_type_label,
                        )}
                        · {versionLabel(detail.definition_version)}
                    </DialogDescription>
                </DialogHeader>
                <div className="flex flex-col gap-3 text-sm">
                    <p data-testid="publish-path-preview">
                        {path || "尚未配置审批人"}
                    </p>
                    <p data-testid="publish-reject-copy">
                        {REJECT_RESTART_COPY}
                    </p>
                    <p className="text-muted-foreground">
                        发布后当前已发布版本将退役。已绑定单据和进行中的审批不受影响。
                    </p>
                </div>
                {error ? (
                    <Alert variant="destructive">
                        <AlertTitle>未能发布</AlertTitle>
                        <AlertDescription>{error}</AlertDescription>
                    </Alert>
                ) : null}
                <DialogFooter>
                    <Button
                        id={`${id}-cancel`}
                        type="button"
                        variant="outline"
                        disabled={publish.isPending}
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                    <Button
                        id={`${id}-confirm`}
                        type="button"
                        disabled={publish.isPending}
                        onClick={() => void handlePublish()}
                    >
                        {publish.isPending ? "正在发布…" : "确认发布"}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
