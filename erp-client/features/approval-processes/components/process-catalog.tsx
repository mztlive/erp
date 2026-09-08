"use client"

import Link from "next/link"
import { Button, buttonVariants } from "@/components/ui/button"
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

/** 单据类型审批配置表，集中展示发布状态、版本及未发布草稿。 */
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
        <Table className="min-w-[680px]">
            <TableHeader>
                <TableRow>
                    <TableHead className="w-[28%]">单据类型</TableHead>
                    <TableHead className="w-[16%]">审批要求</TableHead>
                    <TableHead>流程配置</TableHead>
                    <TableHead className="text-right">操作</TableHead>
                </TableRow>
            </TableHeader>
            <TableBody>
                {items.map((item) => {
                    const noWrite = item.approval_requirement === "NO_APPROVAL"
                    const actions = visibleActions(item, permissions)
                    const segment = toAutomationIdSegment(item.document_type)
                    const href = `/system/approval-processes/${item.document_type}`
                    const label = documentTypeLabel(
                        item.document_type,
                        item.document_type_label,
                    )
                    return (
                        <TableRow
                            key={item.document_type}
                            data-document-type={item.document_type}
                            data-blocked={
                                item.approval_requirement ===
                                    "PROCESS_REQUIRED" &&
                                item.configuration_status ===
                                    "MISSING_CONFIGURATION"
                                    ? "true"
                                    : "false"
                            }
                        >
                            <TableCell>
                                {noWrite ? (
                                    label
                                ) : (
                                    <Link
                                        id={`${id}-row-${segment}-open`}
                                        href={href}
                                        className="font-medium hover:underline focus-visible:rounded-sm focus-visible:outline-2 focus-visible:outline-offset-4"
                                    >
                                        {label}
                                    </Link>
                                )}
                            </TableCell>
                            <TableCell>
                                <span className="text-muted-foreground">
                                    {approvalRequirementLabel(
                                        item.approval_requirement,
                                    )}
                                </span>
                            </TableCell>
                            <TableCell>
                                {noWrite ? (
                                    <span className="text-muted-foreground">
                                        无需配置
                                    </span>
                                ) : (
                                    <div className="space-y-1">
                                        <div className="flex items-center gap-2">
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
                                            {item.published_version ? (
                                                <span className="text-xs text-muted-foreground">
                                                    {versionLabel(
                                                        item.published_version,
                                                    )}
                                                </span>
                                            ) : null}
                                        </div>
                                        {item.draft_version ? (
                                            <p className="text-xs text-muted-foreground">
                                                草稿{" "}
                                                {versionLabel(
                                                    item.draft_version,
                                                )}{" "}
                                                · 尚未发布
                                            </p>
                                        ) : null}
                                    </div>
                                )}
                            </TableCell>
                            <TableCell className="text-right">
                                {noWrite ? (
                                    <span className="text-muted-foreground">
                                        —
                                    </span>
                                ) : (
                                    <div className="flex items-center justify-end gap-1">
                                        {actions.includes("REPLACE_NODES") ? (
                                            <Button
                                                id={`${id}-row-${segment}-continue`}
                                                variant="outline"
                                                size="sm"
                                                onClick={() =>
                                                    onContinueDraft(item)
                                                }
                                            >
                                                继续编辑
                                            </Button>
                                        ) : (
                                            <>
                                                {actions.includes(
                                                    "CREATE_DRAFT",
                                                ) ? (
                                                    <Button
                                                        id={`${id}-row-${segment}-create-draft`}
                                                        variant={
                                                            item.published_version
                                                                ? "ghost"
                                                                : "outline"
                                                        }
                                                        size="sm"
                                                        onClick={() =>
                                                            onCreateDraft(item)
                                                        }
                                                    >
                                                        {item.published_version
                                                            ? "创建新草稿"
                                                            : "配置流程"}
                                                    </Button>
                                                ) : null}
                                                {item.published_version ||
                                                !actions.includes(
                                                    "CREATE_DRAFT",
                                                ) ? (
                                                    <Link
                                                        id={`${id}-row-${segment}-view`}
                                                        href={href}
                                                        className={buttonVariants(
                                                            {
                                                                variant:
                                                                    "ghost",
                                                                size: "sm",
                                                            },
                                                        )}
                                                    >
                                                        查看流程
                                                    </Link>
                                                ) : null}
                                            </>
                                        )}
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
