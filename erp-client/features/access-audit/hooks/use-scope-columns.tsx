"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"
import { EyeIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import type { ScopeRow } from "@/features/access-audit/types"
import { riskLabel } from "@/features/access-audit/lib/risk-labels"

type UseScopeColumnsInput = {
    rowFocusRef: { current: Map<string, HTMLButtonElement | null> }
    openExplain: (type: "ROLE" | "USER", id: string) => void
}

function useScopeColumns({ rowFocusRef, openExplain }: UseScopeColumnsInput) {
    return React.useMemo<ColumnDef<ScopeRow>[]>(
        () => [
            {
                id: "subject",
                header: "主体",
                cell: ({ row }) => (
                    <div>
                        <div className="font-medium">
                            {row.original.subjectLabel}
                        </div>
                        <div className="text-xs text-muted-foreground">
                            {row.original.subjectType === "ROLE"
                                ? "角色"
                                : "用户"}
                        </div>
                    </div>
                ),
            },
            {
                id: "type",
                header: "范围类型",
                cell: ({ row }) => row.original.scopeTypeLabel,
            },
            {
                id: "targets",
                header: "范围对象",
                cell: ({ row }) => row.original.scopeTargets,
            },
            {
                id: "risk",
                header: "风险",
                cell: ({ row }) =>
                    row.original.riskFlags.length
                        ? row.original.riskFlags
                              .map((f) => riskLabel(f))
                              .join("、")
                        : "—",
            },
            {
                id: "actions",
                header: "操作",
                cell: ({ row }) => (
                    <div className="flex justify-end">
                        <Button
                            type="button"
                            size="xs"
                            variant="ghost"
                            ref={(el) => {
                                rowFocusRef.current.set(row.original.id, el)
                            }}
                            onClick={() =>
                                openExplain(
                                    row.original.subjectType,
                                    row.original.subjectId,
                                )
                            }
                        >
                            <EyeIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            有效权限
                        </Button>
                    </div>
                ),
            },
        ],
        [openExplain, rowFocusRef],
    )
}

export { useScopeColumns }
