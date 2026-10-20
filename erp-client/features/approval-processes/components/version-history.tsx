"use client"

import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { definitionStatusLabel, versionLabel } from "../labels"
import type { DefinitionVersionItem } from "../types"

/** 历史版本选择条，选中版本在下方使用同一流程图展示。 */
export function VersionHistory({
    versions,
    selectedVersion,
    onSelect,
    id = "governance-approval-processes-version-history",
}: {
    versions: readonly DefinitionVersionItem[]
    selectedVersion?: string
    onSelect: (item: DefinitionVersionItem) => void
    id?: string
}) {
    if (!versions.length)
        return (
            <p className="p-6 text-sm text-muted-foreground">
                还没有历史版本。
            </p>
        )
    return (
        <div
            className="flex flex-wrap items-center gap-2 px-4 py-4 sm:px-6"
            role="group"
            aria-label="选择历史版本"
        >
            <span className="mr-2 text-xs text-muted-foreground">版本记录</span>
            {versions.map((item) => {
                const selected =
                    String(item.definition_version) === selectedVersion
                return (
                    <Button
                        key={item.definition_id}
                        id={`${id}-version-${toAutomationIdSegment(item.definition_version)}-view`}
                        type="button"
                        size="sm"
                        variant={selected ? "secondary" : "ghost"}
                        aria-pressed={selected}
                        onClick={() => onSelect(item)}
                    >
                        <span className="num">
                            {versionLabel(item.definition_version)}
                        </span>
                        <span className="text-xs text-muted-foreground">
                            {definitionStatusLabel(item.status)}
                        </span>
                    </Button>
                )
            })}
        </div>
    )
}
