"use client"

import Link from "next/link"

import { PageScaffold } from "@/components/business"
import { ListWorkspaceHeader } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { useBookletsQuery } from "@/features/sales-selection/queries"
import {
    BOOKLET_STATUS_LABEL,
    FORM_LABEL,
    SUBMIT_MODE_LABEL,
} from "@/features/sales-selection/types"

export function BookletsListPage() {
    const query = useBookletsQuery({})
    const items = query.data?.items ?? []
    return (
        <PageScaffold density="compact">
            <ListWorkspaceHeader
                eyebrow="销售"
                title="选品册"
                description="查看发给客户的选品册和已提交方案。"
            />
            <div className="overflow-x-auto rounded-xl border">
                <table
                    className="w-full text-sm"
                    id="sales-selection-booklets-table"
                >
                    <thead className="bg-muted/40 text-left">
                        <tr>
                            <th className="px-3 py-2">客户</th>
                            <th className="px-3 py-2">形态</th>
                            <th className="px-3 py-2">提交方式</th>
                            <th className="px-3 py-2">状态</th>
                            <th className="px-3 py-2" aria-label="操作" />
                        </tr>
                    </thead>
                    <tbody>
                        {items.map((item) => (
                            <tr key={item.id} className="border-t">
                                <td className="px-3 py-2">
                                    {item.customer_name}
                                </td>
                                <td className="px-3 py-2">
                                    {FORM_LABEL[item.form]}
                                </td>
                                <td className="px-3 py-2">
                                    {SUBMIT_MODE_LABEL[item.submit_mode]}
                                </td>
                                <td className="px-3 py-2">
                                    {BOOKLET_STATUS_LABEL[item.status]}
                                </td>
                                <td className="px-3 py-2">
                                    <Button
                                        id={`sales-selection-open-${item.id}`}
                                        nativeButton={false}
                                        render={
                                            <Link
                                                href={`/sales/selections/${item.id}`}
                                            />
                                        }
                                        variant="ghost"
                                        size="sm"
                                    >
                                        打开
                                    </Button>
                                    {item.proposal_id ? (
                                        <Button
                                            id={`sales-selection-proposal-${item.id}`}
                                            nativeButton={false}
                                            render={
                                                <Link
                                                    href={`/sales/proposals/${item.proposal_id}`}
                                                />
                                            }
                                            variant="ghost"
                                            size="sm"
                                        >
                                            方案
                                        </Button>
                                    ) : null}
                                </td>
                            </tr>
                        ))}
                    </tbody>
                </table>
            </div>
        </PageScaffold>
    )
}
