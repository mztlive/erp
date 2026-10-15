"use client"

import { useState } from "react"
import { FileUpload, imagePreviewSource } from "@/components/ui/file-upload"
import { Label } from "@/components/ui/label"

export function BrandLogoField({
    id,
    value,
    previewUrl,
    disabled,
    onChange,
    onFilesSelected,
}: {
    id: string
    value: string
    previewUrl: string
    disabled: boolean
    onChange: (value: string) => void
    onFilesSelected: (files: File[]) => void
}) {
    const [file, setFile] = useState<File | null>(null)
    return (
        <div className="space-y-2">
            <Label htmlFor={`${id}-input`}>品牌 Logo（选填）</Label>
            <FileUpload
                idPrefix={id}
                density="compact"
                label="上传品牌 Logo"
                description="建议使用 1:1 方形图片，支持 JPG、PNG、WebP"
                accept="image/jpeg,image/png,image/webp"
                multiple={false}
                disabled={disabled}
                previewSelectedImage
                selectedImageFile={file?.name === value ? file : null}
                previewFit="contain"
                preview={
                    value
                        ? {
                              src: previewUrl || imagePreviewSource(value),
                              name: value,
                              status: "uploaded",
                          }
                        : null
                }
                onPreviewRemove={() => {
                    setFile(null)
                    onChange("")
                }}
                onFilesSelected={(files) => {
                    if (!files[0]) return
                    setFile(files[0])
                    onFilesSelected(files)
                    onChange(files[0].name)
                }}
            />
        </div>
    )
}
