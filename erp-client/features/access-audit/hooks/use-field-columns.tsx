"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import type {
    AccessChangeCommand,
    AccessGovernancePolicyView,
    AccessListView,
    FieldPolicyRow,
} from "@/features/access-audit/types"

type UseFieldColumnsInput = {
    data?: AccessListView
    policies?: AccessGovernancePolicyView
    startChange: (command: AccessChangeCommand) => Promise<void>
}

function useFieldColumns({
    data,
    policies,
    startChange,
}: UseFieldColumnsInput) {
    return React.useMemo<ColumnDef<FieldPolicyRow>[]>(
        () => [
            {
                id: "target",
                header: "策略目标",
                cell: ({ row }) => (
                    <div>
                        <div className="font-medium">
                            {row.original.targetLabel}
                        </div>
                        <div className="font-mono text-xs text-muted-foreground">
                            {row.original.policyTargetId}
                        </div>
                    </div>
                ),
            },
            {
                id: "subject",
                header: "适用",
                cell: ({ row }) => row.original.subjectLabel,
            },
            {
                id: "caps",
                header: "访问能力",
                cell: ({ row }) =>
                    data?.emptyReason === "FIELD_MASKED"
                        ? "****"
                        : row.original.capabilitySummary,
            },
            {
                id: "mode",
                header: "可编辑",
                cell: ({ row }) =>
                    row.original.editable ? (
                        <Badge variant="success">可调整</Badge>
                    ) : (
                        <Badge variant="default">只读</Badge>
                    ),
            },
            {
                id: "actions",
                header: "操作",
                cell: ({ row }) =>
                    row.original.editable ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => {
                                const gp = policies?.fieldPolicyGranularity
                                if (!gp || gp.state !== "CONFIGURED") return
                                void startChange({
                                    subjectType: "FIELD_POLICY",
                                    subjectId: row.original.id,
                                    action: "UPDATE_FIELD_POLICY",
                                    granularityPolicyVersion: gp.policyVersion,
                                    policyTargetId: row.original.policyTargetId,
                                    accessCapabilities: ["MASKED", "VISIBLE"],
                                    expectedPermissionVersion:
                                        data?.permissionVersion ??
                                        row.original.permissionVersion,
                                    reasonCode: "SECURITY_OPS",
                                    idempotencyKey: "pending",
                                })
                            }}
                        >
                            调整能力
                        </Button>
                    ) : (
                        <span className="text-xs text-muted-foreground">
                            策略缺失时只读
                        </span>
                    ),
            },
        ],
        [data?.emptyReason, data?.permissionVersion, policies, startChange],
    )
}

export { useFieldColumns }
