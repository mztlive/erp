"use client"

import { Button } from "@/components/ui/button"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"

import { definitionStatusLabel, versionLabel } from "../labels"
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
            <p
                className="text-sm text-muted-foreground"
                data-testid="version-history-empty"
            >
                还没有历史版本。
            </p>
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
                {versions.map((item) => (
                    <TableRow
                        key={item.definition_id}
                        data-selected={
                            item.definition_version === selectedVersion
                        }
                    >
                        <TableCell>
                            {versionLabel(item.definition_version)}
                        </TableCell>
                        <TableCell>
                            {definitionStatusLabel(item.status)}
                        </TableCell>
                        <TableCell>{item.name}</TableCell>
                        <TableCell className="text-right">
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                onClick={() => onSelect(item)}
                            >
                                查看
                            </Button>
                        </TableCell>
                    </TableRow>
                ))}
            </TableBody>
        </Table>
    )
}
