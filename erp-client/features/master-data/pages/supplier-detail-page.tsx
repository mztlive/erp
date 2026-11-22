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
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceSkeleton,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { SupplierEditorForm } from "@/features/master-data/components/supplier/supplier-editor-form"
import { useSupplierEditor } from "@/features/master-data/hooks/use-supplier-editor"
import { masterDataCopy } from "@/features/master-data/lib/copy"

export function SupplierDetailPage({ stableId }: { stableId: string }) {
    const editor = useSupplierEditor(stableId)
    const { isCreate, detailQuery, data, listHref, accountQuery, canCreate } =
        editor
    const backLink = {
        id: "master-data-supplier-detail-back-list",
        label: "供应商列表",
        href: listHref,
    } as const

    if (isCreate && accountQuery.isPending) {
        return <DetailWorkspaceSkeleton title="新建供应商" />
    }

    if (isCreate && accountQuery.isError) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="新建供应商" back={backLink} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection>
                        <BusinessFailureState
                            error={accountQuery.error}
                            onRetry={() => void accountQuery.refetch()}
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (isCreate && !canCreate) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="新建供应商" back={backLink} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection>
                        <BusinessFailureState
                            kind="permission"
                            description="当前账号没有创建供应商的权限，请联系管理员或有权限的同事。"
                            action={
                                <Button
                                    id="master-data-supplier-detail-back-list-action"
                                    render={<Link href={listHref} />}
                                >
                                    {masterDataCopy.actionBackList}
                                </Button>
                            }
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (!isCreate && detailQuery.isPending) {
        return (
            <DetailWorkspaceSkeleton
                title="供应商详情"
                description={masterDataCopy.centerLoading}
            />
        )
    }

    if (!isCreate && (detailQuery.isError || !data)) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="供应商详情" back={backLink} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection>
                        <BusinessFailureState
                            error={
                                detailQuery.isError
                                    ? detailQuery.error
                                    : undefined
                            }
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
                                        onClick={() =>
                                            void detailQuery.refetch()
                                        }
                                    >
                                        重试
                                    </Button>
                                ) : (
                                    <Button
                                        id="master-data-supplier-detail-back-list-action"
                                        render={<Link href={listHref} />}
                                    >
                                        {masterDataCopy.actionBackList}
                                    </Button>
                                )
                            }
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    return (
        <editor.form.Subscribe selector={(state) => state.isSubmitting}>
            {(isSubmitting) => (
                <SupplierEditorForm
                    idPrefix="master-data-supplier-detail-form"
                    editor={{
                        ...editor,
                        pending: editor.pending || isSubmitting,
                    }}
                />
            )}
        </editor.form.Subscribe>
    )
}
