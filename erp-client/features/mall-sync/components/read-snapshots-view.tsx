"use client"

import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessTableFrame,
    DataTable,
    surfacePanelClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import type {
    MallSnapshotRow,
    MallSyncPageView,
} from "@/features/mall-sync/types"
import { versionText } from "@/lib/ui-text"

type MallSyncSnapshotsViewProps = {
    data: MallSyncPageView | undefined
    snapshotColumns: ColumnDef<MallSnapshotRow>[]
}

export function MallSyncSnapshotsView({
    data,
    snapshotColumns,
}: MallSyncSnapshotsViewProps) {
    return (
        <div className="grid gap-4 xl:grid-cols-[minmax(0,1.3fr)_minmax(18rem,1fr)]">
            <BusinessTableFrame
                title="来源数据"
                description="仅白名单商业字段。不含玩法、卡号、卡密、绑定手机、连接或密钥。"
                table={
                    <DataTable
                        id="mall-sync-snapshots-table"
                        data={data?.snapshots ?? []}
                        columns={snapshotColumns}
                        getRowId={(r) => r.snapshotId}
                        rowCount={(data?.snapshots ?? []).length}
                        layout="flush"
                    />
                }
            />
            {data?.selectedSnapshot ? (
                <Card size="sm" className={surfacePanelClassName}>
                    <CardHeader className="border-b border-grid">
                        <CardTitle className="font-mono text-base">
                            {data.selectedSnapshot.externalOrderNo}
                        </CardTitle>
                        <CardDescription>
                            {versionText.version}{" "}
                            {data.selectedSnapshot.contentHashShort} · 任务{" "}
                            {data.selectedSnapshot.syncJobNo}
                        </CardDescription>
                    </CardHeader>
                    <CardContent className="space-y-2">
                        <Badge variant="outline">
                            {data.selectedSnapshot.mappingStatusLabel}
                        </Badge>
                        {data.selectedSnapshot.conflictFlags.length > 0 ? (
                            <Alert variant="warning">
                                <AlertTitle>冲突标记</AlertTitle>
                                <AlertDescription>
                                    {data.selectedSnapshot.conflictFlags.join(
                                        "、",
                                    )}
                                </AlertDescription>
                            </Alert>
                        ) : null}
                        <dl className="space-y-1.5 text-sm">
                            {data.selectedSnapshot.whitelistFields.map((f) => (
                                <div
                                    key={f.field}
                                    className="flex justify-between gap-2 border-b border-dashed border-grid py-1"
                                >
                                    <dt className="text-muted-foreground">
                                        {f.label}
                                    </dt>
                                    <dd className="text-right font-medium">
                                        {f.value}
                                    </dd>
                                </div>
                            ))}
                        </dl>
                    </CardContent>
                </Card>
            ) : (
                <BusinessEmptyState
                    kind="no-data"
                    title="选择结果"
                    description="从左侧列表选择一条记录"
                    className="rounded-lg border-0 bg-transparent shadow-none ring-0"
                />
            )}
        </div>
    )
}
