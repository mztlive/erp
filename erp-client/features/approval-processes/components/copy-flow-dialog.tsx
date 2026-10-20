"use client"

import * as React from "react"
import { CopyIcon } from "lucide-react"
import { OptionCombobox } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import {
    useDefinitionCatalogQuery,
    useDefinitionDetailQuery,
    useDefinitionVersionsQuery,
} from "../queries"
import { definitionErrorMessage } from "../errors"
import {
    definitionStatusLabel,
    documentTypeLabel,
    versionLabel,
} from "../labels"
import { toEditorNode } from "../draft-nodes"
import type { DefinitionDetailView, DocumentType } from "../types"
import { DefinitionFlowchart } from "./definition-flowchart"

/** 从有权限读取的其它类型选择具体版本，确认后仅带入编辑表单。 */
export function CopyFlowDialog({
    id,
    targetType,
    open,
    onOpenChange,
    onCopy,
    replacesNodes = false,
}: {
    id: string
    targetType: DocumentType
    open: boolean
    onOpenChange: (open: boolean) => void
    onCopy: (source: DefinitionDetailView) => void
    replacesNodes?: boolean
}) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                closeButtonId={`${id}-close`}
                className="max-h-[90dvh] overflow-y-auto sm:max-w-2xl"
            >
                <DialogHeader>
                    <DialogTitle>从其它审批流复制</DialogTitle>
                    <DialogDescription>
                        复制节点顺序、名称、审批人和抄送规则，替换当前编辑内容；保存时按目标流程重新校验人员资格。
                    </DialogDescription>
                </DialogHeader>
                {open && (
                    <SourceFlowPicker
                        id={id}
                        targetType={targetType}
                        replacesNodes={replacesNodes}
                        onCancel={() => onOpenChange(false)}
                        onCopy={(source) => {
                            onCopy(source)
                            onOpenChange(false)
                        }}
                    />
                )}
            </DialogContent>
        </Dialog>
    )
}

function SourceFlowPicker({
    id,
    targetType,
    onCopy,
    onCancel,
    replacesNodes,
}: {
    id: string
    targetType: DocumentType
    onCopy: (source: DefinitionDetailView) => void
    onCancel: () => void
    replacesNodes: boolean
}) {
    // 选择仅用于浏览来源；确认前不修改目标草稿。
    const [sourceType, setSourceType] = React.useState<DocumentType | null>(
        null,
    )
    const [versionId, setVersionId] = React.useState<string | null>(null)
    const catalog = useDefinitionCatalogQuery()
    const sources = (catalog.data ?? []).filter(
        (item) =>
            item.document_type !== targetType &&
            item.approval_requirement === "PROCESS_REQUIRED" &&
            (item.published_version || item.draft_version),
    )
    const versions = useDefinitionVersionsQuery(sourceType)
    const available =
        versions.isPlaceholderData || versions.isError
            ? []
            : (versions.data ?? [])
    const selected =
        available.find((item) => item.definition_id === versionId) ??
        available.find((item) => item.status === "PUBLISHED") ??
        available.find((item) => item.status === "DRAFT") ??
        available[0]
    const detail = useDefinitionDetailQuery(selected?.definition_id ?? null)
    const source =
        !detail.isPlaceholderData &&
        !detail.isError &&
        detail.data?.document_type === sourceType &&
        detail.data?.definition_id === selected?.definition_id
            ? detail.data
            : null
    const error = catalog.error ?? versions.error ?? detail.error
    const retry = () => {
        if (catalog.isError) void catalog.refetch()
        else if (versions.isError) void versions.refetch()
        else void detail.refetch()
    }
    return (
        <>
            <div className="grid gap-4 sm:grid-cols-2">
                <div className="space-y-2">
                    <Label htmlFor={`${id}-source-trigger`}>来源审批流</Label>
                    <OptionCombobox
                        id={`${id}-source`}
                        aria-label="来源审批流"
                        value={sourceType}
                        loading={catalog.isFetching}
                        options={sources.map((item) => ({
                            value: item.document_type,
                            label: documentTypeLabel(
                                item.document_type,
                                item.document_type_label,
                            ),
                        }))}
                        placeholder="选择其它审批流"
                        emptyLabel="暂无可复制的审批流"
                        onValueChange={(value) => {
                            setSourceType(
                                sources.find(
                                    (item) => item.document_type === value,
                                )?.document_type ?? null,
                            )
                            setVersionId(null)
                        }}
                    />
                </div>
                <div className="space-y-2">
                    <Label htmlFor={`${id}-version-trigger`}>来源版本</Label>
                    <OptionCombobox
                        id={`${id}-version`}
                        aria-label="来源版本"
                        value={selected?.definition_id ?? null}
                        options={available.map((item) => ({
                            value: item.definition_id,
                            label: `${versionLabel(item.definition_version)} · ${definitionStatusLabel(item.status)}`,
                        }))}
                        loading={versions.isFetching}
                        disabled={!sourceType}
                        allowClear={false}
                        placeholder="选择版本"
                        onValueChange={setVersionId}
                    />
                </div>
            </div>
            {error ? (
                <div
                    role="alert"
                    className="flex items-center justify-between gap-3 rounded-lg border border-destructive/20 bg-destructive/5 p-3 text-sm"
                >
                    <span>{definitionErrorMessage(error)}</span>
                    <Button
                        id={`${id}-retry`}
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={retry}
                    >
                        重试
                    </Button>
                </div>
            ) : source ? (
                <DefinitionFlowchart
                    id={`${id}-preview`}
                    nodes={[...source.nodes]
                        .sort((a, b) => a.display_order - b.display_order)
                        .map(toEditorNode)}
                    compact
                />
            ) : (
                <div
                    role="status"
                    className="rounded-lg border border-dashed border-border px-4 py-12 text-center text-sm text-muted-foreground"
                >
                    {sourceType
                        ? versions.isSuccess &&
                          !versions.isPlaceholderData &&
                          available.length === 0
                            ? "该流程暂无可读取版本，请选择其它流程"
                            : "正在加载流程…"
                        : "选择来源，预览将复制的流程"}
                </div>
            )}
            <p className="text-xs leading-5 text-muted-foreground">
                {replacesNodes
                    ? "带入后将替换当前编辑内容，点击保存草稿后才生效。"
                    : "来源流程保持不变，复制后可继续调整。"}
            </p>
            <DialogFooter>
                <Button
                    id={`${id}-cancel`}
                    type="button"
                    variant="outline"
                    onClick={onCancel}
                >
                    取消
                </Button>
                <Button
                    id={`${id}-confirm`}
                    type="button"
                    disabled={!source?.nodes.length || Boolean(error)}
                    onClick={() => {
                        if (source) onCopy(source)
                    }}
                >
                    <CopyIcon data-icon="inline-start" />
                    {replacesNodes ? "替换并带入" : "使用此流程"}
                </Button>
            </DialogFooter>
        </>
    )
}
