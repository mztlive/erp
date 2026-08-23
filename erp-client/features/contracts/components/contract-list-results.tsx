"use client"

import Link from "next/link"

import { FormalActionResult } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { ContractActionResult } from "@/features/contracts/hooks/use-contract-list-actions"
import type { ContractExportJob } from "@/features/contracts/types"

type ContractListResultsProps = {
    actionResult: ContractActionResult | null
    exportJob: ContractExportJob | null
}

/** 列表页动作结果条：上传归档反馈 + 导出反馈。 */
export function ContractListResults({
    actionResult,
    exportJob,
}: ContractListResultsProps) {
    return (
        <>
            {actionResult ? (
                <FormalActionResult
                    status={actionResult.status}
                    title={actionResult.title}
                    description={actionResult.description}
                    facts={actionResult.facts}
                    actions={
                        <>
                            {actionResult.nextHref ? (
                                <Button
                                    type="button"
                                    size="sm"
                                    render={
                                        <Link href={actionResult.nextHref} />
                                    }
                                >
                                    查看详情
                                </Button>
                            ) : null}
                            {actionResult.createSalesOrderHref ? (
                                <Button
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    render={
                                        <Link
                                            href={
                                                actionResult.createSalesOrderHref
                                            }
                                        />
                                    }
                                >
                                    新建销售单
                                </Button>
                            ) : null}
                        </>
                    }
                />
            ) : null}

            {exportJob ? (
                <FormalActionResult
                    status="succeeded"
                    title="合同导出完成"
                    description={`共 ${exportJob.rowCount} 条，内容按当前筛选生成；下载时将重新校验权限。`}
                    facts={[
                        { label: "文件", value: exportJob.downloadLabel },
                        { label: "行数", value: String(exportJob.rowCount) },
                    ]}
                />
            ) : null}
        </>
    )
}
