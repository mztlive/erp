import { cp, mkdir, readFile } from "node:fs/promises"

// 与 PDF.js 版本一起发布，中文 CMap、字体和图片解码不依赖外部 CDN。
const source = new URL("../node_modules/pdfjs-dist/", import.meta.url)
const { version } = JSON.parse(
    await readFile(new URL("package.json", source), "utf8"),
)
const destination = new URL(`../public/pdfjs/${version}/`, import.meta.url)
await mkdir(destination, { recursive: true })
await Promise.all(
    ["cmaps", "standard_fonts", "wasm", "iccs"].map((name) =>
        cp(new URL(name, source), new URL(name, destination), {
            recursive: true,
        }),
    ),
)
