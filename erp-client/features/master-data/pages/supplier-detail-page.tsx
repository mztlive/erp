"use client"

/**
 * 供应商详情页 = 查看 + 编辑（同一页面）。
 * - /master-data/suppliers/new  新建
 * - /master-data/suppliers/:id  查看并直接改，保存即形成新版本
 * 不使用侧边 sheet，也没有单独的编辑弹窗。
 */

import Link from "next/link"

import {
    BusinessFailureState,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { SupplierEditorForm } from "@/features/master-data/components/supplier/supplier-editor-form"
import { useSupplierEditor } from "@/features/master-data/hooks/use-supplier-editor"
import { masterDataCopy } from "@/features/master-data/lib/copy"

export function SupplierDetailPage({ stableId }: { stableId: string }) {
    const editor = useSupplierEditor(stableId)
    const { isCreate, detailQuery, data, listHref } = editor

    if (!isCreate && detailQuery.isPending) {
        return (
            <PageScaffold density="compact">
                <PageHeader
                    title="供应商详情"
                    description={masterDataCopy.centerLoading}
                />
                <div
                    className="h-40 animate-pulse rounded-lg bg-muted"
                    aria-busy
                />
            </PageScaffold>
        )
    }

    if (!isCreate && (detailQuery.isError || !data)) {
        return (
            <PageScaffold density="compact">
                <PageHeader title="供应商详情" />
                <BusinessFailureState
                    error={detailQuery.isError ? detailQuery.error : undefined}
                    description={
                        detailQuery.isError
                            ? masterDataCopy.centerLoadFail
                            : masterDataCopy.centerMissingDesc
                    }
                    action={
                        detailQuery.isError ? (
                            <Button
                                id="master-data-supplier-detail-retry"
                                type="button"
                                onClick={() => void detailQuery.refetch()}
                            >
                                重试
                            </Button>
                        ) : (
                            <Button
                                id="master-data-supplier-detail-back-list"
                                render={<Link href={listHref} />}
                            >
                                {masterDataCopy.actionBackList}
                            </Button>
                        )
                    }
                />
            </PageScaffold>
        )
    }

    return (
        <SupplierEditorForm
            idPrefix="master-data-supplier-detail-form"
            editor={editor}
        />
    )
}
