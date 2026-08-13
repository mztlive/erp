/**
 * 文件资产（D05）公共 API 入口。
 * 实现已按职责移入 api/，本文件仅做兼容再导出，对外导出名保持不变。
 */

export {
    uploadFileAssetImage,
    fetchFileAssetPreviewBlob,
    downloadFileAsset,
} from "./api/file-assets"
