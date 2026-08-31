"use client"

import Link from "next/link"

import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"

import { toAutomationIdSegment } from "@/lib/automation-id"

import {
    approvalRequirementLabel,
    configurationStatusLabel,
    configurationStatusTone,
    documentTypeLabel,
    versionLabel,
} from "../labels"
import { canPerformCatalogAction } from "../permissions"
import type { DefinitionAllowedAction, DefinitionCatalogItem } from "../types"

/**
 * 固定单据类型目录表。NO_APPROVAL 无写入口；配置缺失显示阻断。
 */
export function ProcessCatalog({
    items,
    permissions,
    onCreateDraft,
    onContinueDraft,
    id = "governance-approval-processes-catalog",
}: {
    items: readonly DefinitionCatalogItem[]
    permissions: readonly string[] | undefined
    onCreateDraft: (item: DefinitionCatalogItem) => void
    onContinueDraft: (item: DefinitionCatalogItem) => void
    id?: string
}) {
    return (
        <Table>
            <TableHeader>
                <TableRow>
                    <TableHead>单据类型</TableHead>
                    <TableHead>审批政策</TableHead>
                    <TableHead>当前版本</TableHead>
                    <TableHead>配置状态</TableHead>
                    <TableHead>草稿</TableHead>
                    <TableHead className="text-right">操作</TableHead>
                </TableRow>
            </TableHeader>
            <TableBody>
                {items.map((item) => {
                    const blocked =
                        item.approval_requirement === "PROCESS_REQUIRED" &&
                        item.configuration_status === "MISSING_CONFIGURATION"
                    const noWrite = item.approval_requirement === "NO_APPROVAL"
                    const actions = visibleActions(item, permissions)
                    const rowSegment = toAutomationIdSegment(item.document_type)
                    return (
                        <TableRow
                            key={item.document_type}
                            data-document-type={item.document_type}
                            data-blocked={blocked ? "true" : "false"}
                        >
                            <TableCell>
                                {noWrite ? (
                                    documentTypeLabel(
                                        item.document_type,
                                        item.document_type_label,
                                    )
                                ) : (
                                    <Link
                                        id={`${id}-row-${rowSegment}-open`}
                                        className="underline-offset-4 hover:underline"
                                        href={`/system/approval-processes/${item.document_type}`}
                                    >
                                        {documentTypeLabel(
                                            item.document_type,
                                            item.document_type_label,
                                        )}
                                    </Link>
                                )}
                            </TableCell>
                            <TableCell>
                                {approvalRequirementLabel(
                                    item.approval_requirement,
                                )}
                            </TableCell>
                            <TableCell>
                                {item.published_version
                                    ? versionLabel(item.published_version)
                                    : item.approval_requirement ===
                                        "PROCESS_REQUIRED"
                                      ? "配置缺失"
                                      : "不适用"}
                            </TableCell>
                            <TableCell>
                                <StatusBadge
                                    label={configurationStatusLabel(
                                        item.configuration_status,
                                        item.approval_requirement,
                                    )}
                                    tone={configurationStatusTone(
                                        item.configuration_status,
                                        item.approval_requirement,
                                    )}
                                />
                            </TableCell>
                            <TableCell>
                                {versionLabel(item.draft_version)}
                            </TableCell>
                            <TableCell className="text-right">
                                {noWrite || actions.length === 0 ? (
                                    <span className="text-muted-foreground">
                                        —
                                    </span>
                                ) : (
                                    <div className="flex justify-end gap-2">
                                        {actions.includes("REPLACE_NODES") ? (
                                            <Button
                                                id={`${id}-row-${rowSegment}-continue`}
                                                type="button"
                                                size="sm"
                                                variant="outline"
                                                onClick={() =>
                                                    onContinueDraft(item)
                                                }
                                            >
                                                继续编辑
                                            </Button>
                                        ) : null}
                                        {actions.includes("CREATE_DRAFT") ? (
                                            <Button
                                                id={`${id}-row-${rowSegment}-create-draft`}
                                                type="button"
                                                size="sm"
                                                onClick={() =>
                                                    onCreateDraft(item)
                                                }
                                            >
                                                {item.published_version
                                                    ? "创建新草稿"
                                                    : "新建草稿"}
                                            </Button>
                                        ) : null}
                                        {!actions.includes("CREATE_DRAFT") &&
                                        !actions.includes("REPLACE_NODES") ? (
                                            <Button
                                                id={`${id}-row-${rowSegment}-view`}
                                                type="button"
                                                size="sm"
                                                variant="outline"
                                                render={
                                                    <Link
                                                        id={`${id}-row-${rowSegment}-view-link`}
                                                        href={`/system/approval-processes/${item.document_type}`}
                                                    />
                                                }
                                            >
                                                查看
                                            </Button>
                                        ) : null}
                                    </div>
                                )}
                            </TableCell>
                        </TableRow>
                    )
                })}
            </TableBody>
        </Table>
    )
}

/**
 * 计算目录行可见动作。NO_APPROVAL 永远为空。
 *
 * @param item 目录行
 * @param permissions 已授予权限
 */
export const visibleActions = (
    item: DefinitionCatalogItem,
    permissions: readonly string[] | undefined,
): DefinitionAllowedAction[] =>
    item.allowed_actions.filter((action) =>
        canPerformCatalogAction(action, item, permissions),
    )
