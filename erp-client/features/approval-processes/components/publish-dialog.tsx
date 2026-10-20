"use client"

import * as React from "react"

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

import {
    definitionErrorMessage,
    isDefinitionVersionConflict,
    newCommandKey,
} from "../errors"
import { documentTypeLabel, rejectionPolicyCopy, versionLabel } from "../labels"
import { toEditorNode } from "../draft-nodes"
import { CopyRuleSummary } from "./copy-rule-editor"
import { DefinitionFlowchart } from "./definition-flowchart"
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
            <DialogContent
                closeButtonId={`${id}-close`}
                className="max-h-[90dvh] overflow-y-auto sm:max-w-xl"
            >
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
                    <div data-testid="publish-path-preview">
                        <DefinitionFlowchart
                            id={`${id}-flow`}
                            nodes={[...detail.nodes]
                                .sort(
                                    (a, b) => a.display_order - b.display_order,
                                )
                                .map(toEditorNode)}
                            compact
                        />
                    </div>
                    <CopyRuleSummary detail={detail} />
                    <p
                        className="text-xs leading-5 text-muted-foreground"
                        data-testid="publish-reject-copy"
                    >
                        {rejectionPolicyCopy(detail.document_type)}
                    </p>
                    <p className="text-xs leading-5 text-muted-foreground">
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
                    <LoadingButton
                        loading={publish.isPending}
                        id={`${id}-confirm`}
                        type="button"
                        disabled={publish.isPending}
                        onClick={() => void handlePublish()}
                    >
                        {publish.isPending ? "正在发布…" : "确认发布"}
                    </LoadingButton>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
