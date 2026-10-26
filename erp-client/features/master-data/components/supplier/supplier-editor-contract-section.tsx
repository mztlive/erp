"use client"

import { DatePicker } from "@/components/ui/date-picker"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
    CredentialGroup,
    FieldShell,
    SectionPanel,
} from "@/features/master-data/components/supplier/supplier-editor-fields"
import type {
    SupplierEditorSectionProps,
    SupplierMediaLookup,
    SupplierRememberMediaFiles,
} from "@/features/master-data/components/supplier/supplier-editor-section-props"
import { MediaListField } from "@/features/master-data/components/shared/media-list-field"
import { masterDataCopy } from "@/features/master-data/lib/copy"

import { parseMediaList } from "@/features/master-data/lib/resource-fields"

const QUALIFICATION_ACCEPT = "image/jpeg,image/png,image/webp,application/pdf"

export function SupplierEditorContractSection({
    values,
    setFieldValue,
    canEdit,
    mediaUrlsFor,
    mediaAssetIdsFor,
    rememberMediaFiles,
}: SupplierEditorSectionProps & {
    mediaUrlsFor: SupplierMediaLookup
    mediaAssetIdsFor: SupplierMediaLookup
    rememberMediaFiles: SupplierRememberMediaFiles
}) {
    const existingFileNames = [
        values.contractFile,
        values.authorizationFile,
        values.qualification,
        values.foodLicense,
        values.legalPersonIdCard,
    ]
        .flatMap(parseMediaList)
        .concat(
            ...[
                "contractFile",
                "authorizationFile",
                "qualification",
                "foodLicense",
                "legalPersonIdCard",
            ].map((key) => Object.keys(mediaAssetIdsFor(key))),
        )
    return (
        <SectionPanel
            title="合同与资质"
            description="附件选填，支持 JPG、PNG、WebP、PDF，可上传多个文件。"
        >
            <div className="space-y-5">
                <CredentialGroup
                    title="采购合同"
                    description="有效期未填写完整时，不能作为有效合同用于采购或供给。"
                >
                    {(values.contractNo || values.contractFile) &&
                        (!values.contractValidFrom ||
                            !values.contractValidTo) && (
                            <p
                                role="status"
                                className="mb-4 text-sm text-warning-soft-foreground"
                            >
                                有效期未核实：请核实并补齐合同起止日期。
                            </p>
                        )}
                    <div className="grid gap-5 lg:grid-cols-2">
                        <div className="space-y-4">
                            <FieldShell>
                                <Label htmlFor="master-data-supplier-contract-no">
                                    {masterDataCopy.fContractNo}
                                </Label>
                                <Input
                                    id="master-data-supplier-contract-no"
                                    value={values.contractNo}
                                    onChange={(e) =>
                                        setFieldValue(
                                            "contractNo",
                                            e.target.value,
                                        )
                                    }
                                    placeholder="合同编号"
                                    disabled={!canEdit}
                                />
                            </FieldShell>
                            <div className="grid gap-4 sm:grid-cols-2">
                                <FieldShell>
                                    <Label>
                                        {masterDataCopy.fContractValidFrom}
                                    </Label>
                                    <DatePicker
                                        id="master-data-supplier-contract-valid-from-picker"
                                        value={
                                            values.contractValidFrom ||
                                            undefined
                                        }
                                        onValueChange={(next) =>
                                            setFieldValue(
                                                "contractValidFrom",
                                                next ?? "",
                                            )
                                        }
                                        disabled={!canEdit}
                                        className="w-full"
                                    />
                                </FieldShell>
                                <FieldShell>
                                    <Label>
                                        {masterDataCopy.fContractValidTo}
                                    </Label>
                                    <DatePicker
                                        id="master-data-supplier-contract-valid-to-picker"
                                        value={
                                            values.contractValidTo || undefined
                                        }
                                        onValueChange={(next) =>
                                            setFieldValue(
                                                "contractValidTo",
                                                next ?? "",
                                            )
                                        }
                                        disabled={!canEdit}
                                        className="w-full"
                                    />
                                </FieldShell>
                            </div>
                        </div>
                        <div className="border-grid lg:border-l lg:pl-5">
                            <MediaListField
                                existingFileNames={existingFileNames}
                                idPrefix="master-data-supplier-contract-contract-file-upload"
                                label={masterDataCopy.fContractFile}
                                compact
                                value={values.contractFile}
                                onChange={(next) =>
                                    setFieldValue("contractFile", next)
                                }
                                urlByFileName={mediaUrlsFor("contractFile")}
                                assetIdByFileName={mediaAssetIdsFor(
                                    "contractFile",
                                )}
                                onFilesSelected={rememberMediaFiles}
                                disabled={!canEdit}
                                accept={QUALIFICATION_ACCEPT}
                            />
                        </div>
                    </div>
                </CredentialGroup>

                <CredentialGroup title="品牌与经营授权">
                    <div className="grid gap-5 lg:grid-cols-2">
                        <div className="grid content-start gap-4 sm:grid-cols-2">
                            <FieldShell>
                                <Label>
                                    {masterDataCopy.fAuthorizationValidFrom}
                                </Label>
                                <DatePicker
                                    id="master-data-supplier-contract-auth-from-picker"
                                    value={
                                        values.authorizationValidFrom ||
                                        undefined
                                    }
                                    onValueChange={(next) =>
                                        setFieldValue(
                                            "authorizationValidFrom",
                                            next ?? "",
                                        )
                                    }
                                    disabled={!canEdit}
                                    className="w-full"
                                />
                            </FieldShell>
                            <FieldShell>
                                <Label>
                                    {masterDataCopy.fAuthorizationValidTo}
                                </Label>
                                <DatePicker
                                    id="master-data-supplier-contract-auth-to-picker"
                                    value={
                                        values.authorizationValidTo || undefined
                                    }
                                    onValueChange={(next) =>
                                        setFieldValue(
                                            "authorizationValidTo",
                                            next ?? "",
                                        )
                                    }
                                    disabled={!canEdit}
                                    className="w-full"
                                />
                            </FieldShell>
                        </div>
                        <div className="border-grid lg:border-l lg:pl-5">
                            <MediaListField
                                existingFileNames={existingFileNames}
                                idPrefix="master-data-supplier-contract-authorization-file-upload"
                                label={masterDataCopy.fAuthorizationFile}
                                compact
                                value={values.authorizationFile}
                                onChange={(next) =>
                                    setFieldValue("authorizationFile", next)
                                }
                                urlByFileName={mediaUrlsFor(
                                    "authorizationFile",
                                )}
                                assetIdByFileName={mediaAssetIdsFor(
                                    "authorizationFile",
                                )}
                                onFilesSelected={rememberMediaFiles}
                                disabled={!canEdit}
                                accept={QUALIFICATION_ACCEPT}
                            />
                        </div>
                    </div>
                </CredentialGroup>

                <CredentialGroup title="企业经营资质">
                    <div className="grid gap-4 lg:grid-cols-3">
                        <div className="rounded-md border border-border bg-background p-4">
                            <MediaListField
                                existingFileNames={existingFileNames}
                                idPrefix="master-data-supplier-contract-qualification-upload"
                                label={masterDataCopy.fQualification}
                                compact
                                value={values.qualification}
                                onChange={(next) =>
                                    setFieldValue("qualification", next)
                                }
                                urlByFileName={mediaUrlsFor("qualification")}
                                assetIdByFileName={mediaAssetIdsFor(
                                    "qualification",
                                )}
                                onFilesSelected={rememberMediaFiles}
                                disabled={!canEdit}
                                accept={QUALIFICATION_ACCEPT}
                            />
                        </div>
                        <div className="rounded-md border border-border bg-background p-4">
                            <MediaListField
                                existingFileNames={existingFileNames}
                                idPrefix="master-data-supplier-contract-food-license-upload"
                                label={masterDataCopy.fFoodLicense}
                                compact
                                value={values.foodLicense}
                                onChange={(next) =>
                                    setFieldValue("foodLicense", next)
                                }
                                urlByFileName={mediaUrlsFor("foodLicense")}
                                assetIdByFileName={mediaAssetIdsFor(
                                    "foodLicense",
                                )}
                                onFilesSelected={rememberMediaFiles}
                                disabled={!canEdit}
                                accept={QUALIFICATION_ACCEPT}
                            />
                        </div>
                        <div className="rounded-md border border-border bg-background p-4">
                            <MediaListField
                                existingFileNames={existingFileNames}
                                idPrefix="master-data-supplier-contract-legal-person-id-card-upload"
                                label={masterDataCopy.fLegalPersonIdCard}
                                compact
                                value={values.legalPersonIdCard}
                                onChange={(next) =>
                                    setFieldValue("legalPersonIdCard", next)
                                }
                                urlByFileName={mediaUrlsFor(
                                    "legalPersonIdCard",
                                )}
                                assetIdByFileName={mediaAssetIdsFor(
                                    "legalPersonIdCard",
                                )}
                                onFilesSelected={rememberMediaFiles}
                                disabled={!canEdit}
                                accept={QUALIFICATION_ACCEPT}
                            />
                        </div>
                    </div>
                </CredentialGroup>
            </div>
        </SectionPanel>
    )
}
