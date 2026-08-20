"use client"

import { BusinessEmptyState } from "@/components/business"
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

import {
    definitionStatusLabel,
    definitionStatusTone,
    versionLabel,
} from "../labels"
import type { DefinitionVersionItem } from "../types"

/**
 * 历史版本列表。历史只读，不得直接改写或重新发布同一版本。
 */
export function VersionHistory({
    versions,
    selectedVersion,
    onSelect,
}: {
    versions: readonly DefinitionVersionItem[]
    selectedVersion?: string
    onSelect: (item: DefinitionVersionItem) => void
}) {
    if (versions.length === 0) {
        return (
            <div data-testid="version-history-empty">
                <BusinessEmptyState
                    kind="no-data"
                    title="还没有历史版本"
                    description="发布后才会留下可查阅的历史版本。"
                    className="rounded-none border-0 bg-transparent p-6 shadow-none ring-0"
                />
            </div>
        )
    }

    return (
        <Table>
            <TableHeader>
                <TableRow>
                    <TableHead>版本</TableHead>
                    <TableHead>状态</TableHead>
                    <TableHead>名称</TableHead>
                    <TableHead className="text-right">操作</TableHead>
                </TableRow>
            </TableHeader>
            <TableBody>
                {versions.map((item) => {
                    const selected = item.definition_version === selectedVersion
                    return (
                        <TableRow
                            key={item.definition_id}
                            data-selected={selected}
                            className={selected ? "bg-accent/50" : undefined}
                        >
                            <TableCell>
                                {versionLabel(item.definition_version)}
                            </TableCell>
                            <TableCell>
                                <StatusBadge
                                    tone={definitionStatusTone(item.status)}
                                    label={definitionStatusLabel(item.status)}
                                />
                            </TableCell>
                            <TableCell>{item.name}</TableCell>
                            <TableCell className="text-right">
                                <Button
                                    type="button"
                                    size="sm"
                                    variant={selected ? "secondary" : "outline"}
                                    onClick={() => onSelect(item)}
                                >
                                    查看
                                </Button>
                            </TableCell>
                        </TableRow>
                    )
                })}
            </TableBody>
        </Table>
    )
}
