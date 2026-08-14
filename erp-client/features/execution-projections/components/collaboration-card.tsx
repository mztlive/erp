"use client"

import Link from "next/link"
import { ExternalLinkIcon, HistoryIcon, RadarIcon } from "lucide-react"

import { DocumentSection } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { CollaborationConsumptionPanel } from "@/features/execution-projections/components/collaboration-consumption-panel"
import { CollaborationProjectionPanel } from "@/features/execution-projections/components/collaboration-projection-panel"
import { useSalesOrderCollaborationQuery } from "@/features/execution-projections/hooks/queries"
import { openWorkspaceLabel } from "@/lib/ui-text"
import { getErrorMessage } from "@/lib/api/errors"

/**
 * W05 协同子区：单张销售单当前执行信息与商城接收状态（只读）。
 * 用户无需先进入 W23 即可看懂协同水位。
 */
export function SalesOrderCollaborationCard({
    salesOrderId,
    salesOrderNo,
}: {
    salesOrderId: string
    salesOrderNo: string
}) {
    const query = useSalesOrderCollaborationQuery(salesOrderId)

    if (query.isPending) {
        return (
            <DocumentSection title="与商城对接" description="正在读取对接情况…">
                <div className="h-24 animate-pulse rounded-xl bg-muted" />
            </DocumentSection>
        )
    }

    if (query.isError || query.data == null) {
        return (
            <DocumentSection title="与商城对接" description="读取对接情况失败">
                <Alert variant="destructive" role="alert">
                    <RadarIcon aria-hidden="true" />
                    <AlertTitle>数据加载失败</AlertTitle>
                    <AlertDescription>
                        {getErrorMessage(
                            query.error,
                            "无法读取执行信息与商城接收状态，请刷新后重试。",
                        )}
                    </AlertDescription>
                </Alert>
            </DocumentSection>
        )
    }

    const data = query.data
    if (!data?.hasProjection) {
        return (
            <DocumentSection
                title="与商城对接"
                description="卡券销售单生效后，系统会自动把信息推给商城。"
            >
                <Alert>
                    <RadarIcon aria-hidden="true" />
                    <AlertTitle>还没推给商城</AlertTitle>
                    <AlertDescription>
                        {data?.note ??
                            "本单生效后会自动生成给商城的信息，不用手工新建。多单汇总可在执行信息里查。"}
                    </AlertDescription>
                </Alert>
                <div className="mt-3">
                    <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        render={
                            <Link
                                href={`/commerce/execution-projections?q=${encodeURIComponent(salesOrderNo)}`}
                            />
                        }
                    >
                        按单号查执行信息
                    </Button>
                </div>
            </DocumentSection>
        )
    }

    return (
        <DocumentSection
            title="与商城对接"
            description="本区只读：看销售是否生效、信息是否发出、商城是否确认。"
            action={
                <div className="flex flex-wrap gap-2">
                    {data.historyHref ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            render={<Link href={data.historyHref} />}
                        >
                            <HistoryIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            查看推送历史
                        </Button>
                    ) : null}
                    {data.w23Href ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            render={<Link href={data.w23Href} />}
                        >
                            <ExternalLinkIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            {openWorkspaceLabel("W23")}
                        </Button>
                    ) : null}
                </div>
            }
        >
            <Alert className="mb-4">
                <RadarIcon aria-hidden="true" />
                <AlertTitle>说明</AlertTitle>
                <AlertDescription>
                    {data.note}
                    商城接收失败不会撤销本单或应收；要改内容请走「发起改单」。
                </AlertDescription>
            </Alert>

            <CollaborationProjectionPanel data={data}>
                <CollaborationConsumptionPanel
                    salesOrderId={salesOrderId}
                    salesOrderNo={salesOrderNo}
                />
            </CollaborationProjectionPanel>
        </DocumentSection>
    )
}
