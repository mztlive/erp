/** 京东授权私钥仅在本次办理中本地读取，不能上传到通用素材存储。 */
export const JD_VOP_PRIVATE_KEY_MAX_BYTES = 32 * 1024

export function jdVopPrivateKeyFileError(file: File): string | null {
    const name = file.name.toLowerCase()
    if (file.size === 0) return "私钥文件为空，请重新选择。"
    if (file.size > JD_VOP_PRIVATE_KEY_MAX_BYTES)
        return "私钥文件不能超过 32 KB，请选择原始私钥文本。"
    if (name.includes(".") && !/\.(pem|key|txt)$/.test(name))
        return "请选择 PEM、KEY、TXT 或无扩展名的私钥文本文件。"
    return null
}

/** 同时接受 PEM 和官方示例使用的纯 Base64 私钥，真实密钥校验交给授权端。 */
export async function readJdVopPrivateKey(file: File): Promise<string> {
    const invalid = jdVopPrivateKeyFileError(file)
    if (invalid) throw new Error(invalid)
    let contents: string
    try {
        contents = (await file.text()).trim()
    } catch {
        throw new Error("无法读取私钥文件，请重新选择可读取的私钥文本。")
    }
    const pem =
        /^-----BEGIN (RSA PRIVATE KEY|PRIVATE KEY)-----\s+[A-Za-z0-9+/=\s]+\s+-----END \1-----$/.test(
            contents,
        )
    const base64 = contents.replace(/\s/g, "")
    const raw = /^[A-Za-z0-9+/]+={0,2}$/.test(base64) && base64.length % 4 === 0
    if (!pem && !raw)
        throw new Error(
            "私钥内容格式不正确，请选择未加密的 RSA 私钥 PEM 或纯 Base64 文本。",
        )
    return contents
}
